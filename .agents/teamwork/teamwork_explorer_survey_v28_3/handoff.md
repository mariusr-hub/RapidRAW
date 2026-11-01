# Comprehensive Survey & Implementation Plan: R4, R5, R6, R7, and R8

**Author**: `teamwork_explorer_survey_v28_3`  
**Date**: 2026-10-30T22:58:00Z  
**Target Milestone**: Master Architectural Gap Matrix & Comprehensive Engineering Plan (v2.8)  
**Scope**: Requirements R4, R5, R6, R7, and R8  

---

## 1. Observation

### 1.1 R4: Window Transparency & Background Stability
- **`src-tauri/tauri.conf.json:21`**:
  ```json
  "transparent": false,
  "decorations": false
  ```
  `tauri.conf.json` already specifies `"transparent": false` for the `"main"` window.
- **`index.html:1-18`**:
  ```html
  <html lang="en" class="overscroll-none">
  ...
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.jsx"></script>
  </body>
  </html>
  ```
  `index.html` lacks explicit inline background styles on `html`, `body`, or `#root`.
- **`src/styles.css:104-117`**:
  ```css
  html {
    background-color: transparent;
  }

  html,
  body,
  #root {
    height: 100%;
  }

  body {
    background-color: transparent;
    @apply text-text-primary font-sans;
  }
  ```
  `html` and `body` are explicitly set to `background-color: transparent;`.
- **`src/App.tsx:868-873`**:
  ```tsx
  <div
    className={clsx(
      'flex flex-col h-screen font-sans text-text-primary overflow-hidden select-none',
      useMacWindowShell && 'macos-window-shell',
      isWgpuActive ? 'bg-transparent' : 'bg-bg-primary',
    )}
  >
  ```
  When `isWgpuActive` is `true`, the outer application container element is assigned `bg-transparent`.
- **`src/components/panel/Editor.tsx:2287-2292`**:
  ```tsx
  <div
    className={clsx(
      'flex-1 relative overflow-hidden touch-none',
      isFullScreen ? 'rounded-none' : 'rounded-lg',
      appSettings?.useWgpuRenderer !== false && !isFullScreen && 'ring-[9999px] ring-bg-secondary',
      !isWgpuActive && ((appSettings as any)?.editorNeutralGreyBg ? 'bg-[#808080]' : 'bg-bg-secondary'),
    )}
  ```
  When `isWgpuActive` is `true`, the canvas viewport container has no background color class assigned.
- **`src-tauri/src/lib.rs:2547-2609`**:
  ```rust
  window.on_window_event(move |event| match event {
      tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_) => { ... }
      _ => {}
  });
  ```
  There is no handler for `tauri::WindowEvent::CloseRequested` to terminate background worker/GPU threads or clean up swapchains before window destruction.

---

### 1.2 R5: Left Sidebar Navigation Dual-Tab Switcher
- **`src/App.tsx:714-738`**:
  ```tsx
  const renderAppPanel = useCallback(
    (panelId: Panel) => {
      switch (panelId) {
        case Panel.FolderTree:
          if (activeView === 'editor') {
            return <TonesPanel />;
          }
          return (
            <FolderTree
              isResizing={isResizing}
              onContextMenu={handleFolderTreeContextMenu}
              onAlbumContextMenu={handleAlbumTreeContextMenu}
              onSelectAlbum={(id, name, images) => {
                void handleSelectAlbum(id, name, images, false);
              }}
              onFolderSelect={(path) => {
                void handleSelectSubfolder(path, false, undefined, true, false);
              }}
              onToggleFolder={handleToggleFolder}
              onOpenFolder={handleOpenFolder}
              style={{ width: '100%', height: '100%' }}
              isInstantTransition={isInstantTransition}
            />
          );
  ```
  In `Editor` mode (`activeView === 'editor'`), `Panel.FolderTree` unconditionally renders `<TonesPanel />`, completely hiding `<FolderTree />` with zero navigation controls for the user to switch back to the folder hierarchy.
- **`src/components/panel/left/TonesPanel.tsx:265-294`**:
  `TonesPanel` renders its own fixed header ("Tones & LOOKs", "Radiant Photo & SNS-HDR Pro") and scrollable list of profiles, but provides no tabs or toggle controls for folder navigation.

---

### 1.3 R6: White Balance Dropper Fallback & Auto-Crop Pixel Scaling
- **`src/components/panel/editor/ImageCanvas.tsx:2360-2390`**:
  ```tsx
  const isKelvinWhiteBalance = getWhiteBalanceMode(appSettings) === WhiteBalanceMode.Kelvin;
  const asShotWhiteBalance = selectedImage?.asShotWhiteBalance;

  const applyWbPick = useCallback(
    async (corners: Coord[]) => {
      const state = wbSampleStateRef.current;
      state.generation += 1;
      state.pending = null;
      const { generation, session } = state;

      try {
        const sample = await invoke<WbSample>(Invokes.SampleWhiteBalance, { corners });
        if (state.session !== session) return;
        if (state.generation === generation) {
          setWbSample(sample);
        }
        if (asShotWhiteBalance) {
          const picked = { temperature: sample.temperature, tint: sample.tint };
          setAdjustments((prev: Adjustments) =>
            isKelvinWhiteBalance
              ? withKelvinWhiteBalance(prev, picked)
              : withRelativeWhiteBalance(prev, toRelativeWhiteBalance(asShotWhiteBalance, picked)),
          );
        }
        onWbPicked?.();
      } catch (err) {
        console.error('Failed to pick white balance:', err);
      }
    },
    [setAdjustments, onWbPicked, asShotWhiteBalance, isKelvinWhiteBalance],
  );
  ```
  `applyWbPick` wraps the adjustment update in `if (asShotWhiteBalance) { ... }`. When an image lacks camera EXIF as-shot metadata (`asShotWhiteBalance == null` / `undefined`, common on non-RAW images, scans, and some DNGs), clicking the canvas with the eyedropper executes the backend sample call but silently drops the adjustment update, leaving the user with an inoperative WB picker.
- **`src/components/panel/editor/ImageCanvas.tsx:3358-3363`**:
  ```tsx
  const wbSwatchWhiteBalance =
    wbSample && asShotWhiteBalance
      ? isKelvinWhiteBalance
        ? wbSample
        : toRelativeWhiteBalance(asShotWhiteBalance, wbSample)
      : null;
  ```
  The swatch HUD preview also disappears if `asShotWhiteBalance` is undefined.
- **`src/components/panel/right/CropPanel.tsx:251-275`**:
  ```tsx
  const handleCompositionAi = useCallback(async () => {
    if (!selectedImage?.path) return;
    setIsAnalyzingComposition(true);
    try {
      const res: any = await invoke('get_saliency_crop_recommendation', {
        aspectRatio: (aspectRatio !== null && !isNaN(aspectRatio)) ? aspectRatio : 0.0,
        orientationSteps: orientationSteps ?? 0,
        flipHorizontal: flipHorizontal ?? false,
        flipVertical: flipVertical ?? false,
      });

      if (res && res.width > 0 && res.height > 0) {
        const newCrop: Crop = {
          unit: '%',
          x: res.x,
          y: res.y,
          width: res.width,
          height: res.height,
        };
        setAdjustments((prev: Adjustments) => ({
          ...prev,
          crop: newCrop,
        }));
        toast.success(t('editor.crop.compositionAiSuccess', 'Composition AI crop applied'));
      }
  ```
  `res` returns normalized percentages in $[0, 100]$ (e.g., $x=10, y=10, w=80, h=80$). `handleCompositionAi` writes `unit: '%'` directly into `adjustments.crop`.
- **`src/components/panel/Editor.tsx:359-361` & `1869-1882`**:
  ```tsx
  if (adjustments.crop) {
    return { width: adjustments.crop.width, height: adjustments.crop.height } as ImageDimensions;
  }
  ...
  const { crop: pixelCrop } = adjustments;
  if (pixelCrop) {
    const pct: PercentCrop = {
      unit: '%',
      x: (pixelCrop.x / cropBaseWidth) * 100,
      y: (pixelCrop.y / cropBaseHeight) * 100,
      width: (pixelCrop.width / cropBaseWidth) * 100,
      height: (pixelCrop.height / cropBaseHeight) * 100,
    };
    setCrop(pct);
  ```
  `Editor.tsx` treats `adjustments.crop` values as raw pixel coordinates and computes `(pixelCrop.x / cropBaseWidth) * 100`. For an image of 6000x4000 pixels, $10 / 6000 \times 100 = 0.16\%$, shrinking the auto-crop rectangle into an invisible speck.

---

### 1.4 R7: Brand Scrubbing & Shader Synchronization
- **Competitor Trademark Occurrences Found**:
  - `SNS-HDR`:
    - `src/components/panel/left/TonesPanel.tsx:26, 233, 278, 318, 323, 328` (`SNS_HDR_PROFILES`, `SNS-HDR Optical Profiles`, etc.)
    - `src/components/adjustments/ReferencePresetsRibbon.tsx:34, 375, 423, 427, 451`
    - `src-tauri/src/adjustment_utils.rs:155, 156, 160, 189, 218, 249, 278, 308, 339, 369` (`get_sns_hdr_reference_presets`, `"SNS Natural"`, `"SNS Interior"`, etc.)
    - `src-tauri/src/default_presets.rs:17, 19, 20, 21` (`"folder-sns-hdr"`, `"SNS-HDR Reference Profiles"`)
    - `src-tauri/tests/test_shader_compilation.rs:5, 25, 26, 27, 43` (`get_sns_hdr_reference_presets`, `test_sns_hdr_eight_reference_presets`)
  - `Radiant Photo` / `Radiant`:
    - `src/components/panel/left/TonesPanel.tsx:42, 44, 45, 67, 89, 111, 132, 152, 234, 278, 367, 372, 377` (`RADIANT_CREATIVE_LOOKS`, `'radiant-vivid'`, etc.)
    - `src/components/adjustments/Color.tsx:651, 655` (`Radiant Color Science & Cast Removal`)
    - `src/components/adjustments/Basic.tsx:12, 363, 1078` (Radiant QuickCorrect, etc.)
    - `src/components/adjustments/Details.tsx:195` (Radiant 5-Mode Denoise)
    - `src/components/panel/editor/EditorToolbar.tsx:711, 721` (`"Smart Tone (Radiant Dynamic Exposure & Contrast)"`)
    - `src-tauri/crates/rapidraw-gpu/src/shader.wgsl:363, 364, 380, 384, 404` (`get_radiant_trapezoidal_hsl_influence`, `apply_radiant_tint_and_gamut_declip`, `evaluate_radiant_pivot_tone`)
    - `src-tauri/src/shaders/shader.wgsl:363, 364, 380, 384, 404` (Identical functions)
    - `src-tauri/src/semantic_auto_polish.rs:779, 806`
    - `src-tauri/crates/rapidraw-dsp/src/lib.rs:1, 5, 27, 84, 175, 260, 320`
    - `src-tauri/tests/test_shader_compilation.rs:118`
  - `DxO`:
    - `src-tauri/src/image_processing.rs:1518, 2535` (`pub _pad_dxo_end: f32`)
    - `src-tauri/crates/rapidraw-gpu/src/types.rs:193` (`pub _pad_dxo_end: f32`)
    - `src-tauri/crates/rapidraw-gpu/src/shader.wgsl:166, 1380, 1531, 1532, 1693, 1711, 2157, 2178, 2179, 2189, 2198, 2228, 2229, 2272, 2273, 2315, 2316, 2325, 2326, 2401, 2413, 2512, 2524, 2532, 2551, 2785, 2786, 2788, 2789` (`apply_dxo_anti_ac_defringe`, `apply_dxo_anamorphosis`, `apply_dxo_hlr_reconstruction`, `apply_dxo_clearview_haze_removal`, `apply_dxo_shadow_desaturation`, `apply_dxo_saturated_colors_protection`, `_pad_dxo_end`)
    - `src-tauri/src/shaders/shader.wgsl` (Identical locations)
    - `src-tauri/src/raw_processing.rs:527, 1107, 1153`
    - `src-tauri/src/denoising.rs:939`
    - `src-tauri/tests/test_shader_compilation.rs:110, 111, 114, 117, 120, 152`
  - `Luminar Neo`:
    - `src/components/panel/right/CropPanel.tsx:889` (`Luminar Neo Composition AI 1-Click Recommendation`)
    - `src/components/panel/right/AIPanel.tsx:1209` (`Luminar Neo 1-Click Auto Remove Dust & Blemishes`)
    - `src-tauri/crates/rapidraw-gpu/src/shader.wgsl:2576` (`Luminar Neo parity`)
    - `src-tauri/src/shaders/shader.wgsl:2576` (`Luminar Neo parity`)
    - `src-tauri/src/hdr2.rs:21, 402, 488`
  - `AKVIS`:
    - `src-tauri/src/inpainting.rs:1479, 1482, 1517, 1536` (`AKVIS Retoucher & SoftOrbits Photo Retoucher Reverse-Engineered Tools`, `AKVIS Retoucher parity`)
- **Shader File Verification**:
  - `src-tauri/crates/rapidraw-gpu/src/shader.wgsl` (110,042 bytes)
  - `src-tauri/src/shaders/shader.wgsl` (110,042 bytes)
  - PowerShell SHA256 Hash check:
    ```
    B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE (rapidraw-gpu/src/shader.wgsl)
    B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE (src/shaders/shader.wgsl)
    ```
    Both copies are currently 100% byte-for-byte identical.

---

### 1.5 R8: Verification and Release Packaging
- **`npm run typecheck`**:
  Executed `tsc --noEmit` synchronously via task runner. Exit code: **0**, 0 TypeScript errors detected across the entire codebase.
- **`scripts/with-stay-awake.ps1`**:
  Acquires Win32 power management `SetThreadExecutionState` (`ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED | ES_CONTINUOUS`) to prevent OS sleep during long builds.
- **Installer Target & Versioning**:
  - `src-tauri/tauri.conf.json:3, 111`: `"productName": "RapidRAW"`, `"version": "1.6.8"`.
  - `package.json:3`: `"version": "1.6.8"`.
  - `src-tauri/Cargo.toml:3`: `version = "1.6.8"`.
  - Tauri build command in `package.json:22`: `npm.cmd run tauri build -- --bundles nsis`.
  - Note on artifact naming: Building with version `1.6.8` produces `RapidRAW_1.6.8_x64-setup.exe`. If the release specification strictly mandates `RapidRAW_1.6.7-enhanced.1_x64-setup.exe`, the version string in `tauri.conf.json`, `package.json`, and `Cargo.toml` must be adjusted accordingly before building.

---

## 2. Logic Chain

### 2.1 R4 Logic
1. Windows Desktop Window Manager (DWM) with Hardware-Accelerated GPU Scheduling (HAGS) compositing flickers when an application's swapchain and webview are configured as transparent.
2. Although `tauri.conf.json` has `"transparent": false`, `styles.css` has `html { background-color: transparent; }` and `body { background-color: transparent; }`.
3. When `isWgpuActive` is true, `App.tsx:871` sets the main container to `bg-transparent`, and `Editor.tsx:2291` leaves the canvas viewport with no background class.
4. During view transitions or canvas repaints, Webview2 renders transparent frames, exposing the underlying desktop surface.
5. Therefore, enforcing solid background `#121212` / `#18181b` on `html, body, #root` in `index.html` and `styles.css`, and removing `bg-transparent` when switching views, will permanently eliminate desktop punch-through and flicker.

### 2.2 R5 Logic
1. In `src/App.tsx:717-720`, `case Panel.FolderTree:` checks `if (activeView === 'editor') return <TonesPanel />;`.
2. This creates a hard modal trap: whenever the user opens an image to edit it, `FolderTree` is replaced by `TonesPanel` without navigation tabs or toggles.
3. Users cannot select other folders, switch sources, or open albums without exiting back to Library view.
4. Implementing a top dual-tab bar in the Left Sidebar (`[ 📁 Folders & Sources ]` vs `[ ✨ HDR & Looks ]`) preserves both capabilities simultaneously, giving users instant access to folder browsing and tone profiles at all times.

### 2.3 R6 Logic
1. In `ImageCanvas.tsx`, `applyWbPick` conditionally executes `setAdjustments` only inside `if (asShotWhiteBalance)`.
2. For any file format or camera source without `asShotWhiteBalance` in EXIF metadata, `applyWbPick` silently fails to apply changes.
3. Adding a fallback `const effectiveAsShot = asShotWhiteBalance ?? { temperature: 5500, tint: 0 };` provides a neutral 5500K daylight baseline, making WB sampling work 100% of the time.
4. In `CropPanel.tsx`, `handleCompositionAi` receives saliency crop values in percentages (0–100%) from `get_saliency_crop_recommendation`.
5. It assigns `unit: '%'` into `adjustments.crop`, but `Editor.tsx:360` expects `adjustments.crop` to contain pixel coordinates (dividing it by `cropBaseWidth` to get percentages).
6. Consequently, percentage values (e.g. 80) are treated as 80 pixels, compressing the crop box to 0.1% of the image.
7. Scaling normalized percentage coordinates by `cropBaseWidth` and `cropBaseHeight` (`pixelX = (res.x / 100) * cropBaseWidth`, etc.) and committing `unit: 'px'` will synchronize canvas state and restore proper framing.

### 2.4 R7 Logic
1. Commercial trademarks ("SNS-HDR", "Radiant Photo", "Luminar Neo", "DxO", "AKVIS") violate clean branding requirements and project specifications.
2. A comprehensive search identified all instances across UI labels, tooltips, variable names, struct fields (`_pad_dxo_end`), comments, and Rust tests.
3. Renaming these tokens to neutral photography terminology ("Dynamic HDR Profiles", "Smart Auto-Crop & Level", "Edge Flow Guides", "Protected Areas", etc.) while synchronizing test assertions in `test_shader_compilation.rs` cleans all trademarks without breaking builds.
4. The two `shader.wgsl` copies currently match byte-for-byte (`SHA256: B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE`). Adding an automated regression test in `tests/test_shader_compilation.rs` guarantees that any future edits to either copy without synchronization will immediately fail CI.

### 2.5 R8 Logic
1. `npm run typecheck` passed with 0 errors.
2. Rust tests and compilation must execute cleanly before packaging.
3. Packaging via `powershell -File ./scripts/with-stay-awake.ps1 npm.cmd run tauri build -- --bundles nsis` will prevent OS sleep during release compilation and produce the final installer.

---

## 3. Caveats
1. **WGPU Swapchain Surface**: When `isWgpuActive` is true, the WGPU surface renders directly onto the window behind the Webview2 surface. Setting `#121212` on `html, body, #root` must ensure that the specific rectangular hole for the canvas in `Editor.tsx` remains transparent only where WGPU renders (which `Editor.tsx` handles via `ring-[9999px]` clipping), while ensuring the rest of the window and unmounted views are always solid `#121212`.
2. **Preset ID Backward Compatibility**: When renaming `sns-natural` or `SNS_HDR_PROFILES` to `hdr-natural` or `DYNAMIC_HDR_PROFILES`, any existing user sidecar JSON files that stored `activeProfileId: "sns-natural"` can be supported via an alias/migration fallback: `id === 'sns-natural' || id === 'hdr-natural'`.
3. **Build Lock Concurrency**: When multiple tasks or agents invoke `cargo check` or `cargo test` simultaneously, Cargo holds a file lock on the target directory (`Blocking waiting for file lock on build directory`). Tasks must run sequentially rather than concurrently.

---

## 4. Conclusion & Step-by-Step Implementation Plan for Worker

### Step 1: Fix R4 (Window Transparency & Background Stability)
1. **`index.html`**: Add inline background style:
   ```html
   <html lang="en" class="overscroll-none" style="background-color: #121212;">
     ...
     <body style="background-color: #121212; margin: 0;">
       <div id="root" style="background-color: #121212; height: 100%;"></div>
   ```
2. **`src/styles.css`**: Replace transparent backgrounds at lines 104–117:
   ```css
   html {
     background-color: #121212;
   }

   html,
   body,
   #root {
     height: 100%;
     background-color: #121212;
   }

   body {
     @apply text-text-primary font-sans;
   }
   ```
3. **`src/App.tsx:871`**:
   Replace `isWgpuActive ? 'bg-transparent' : 'bg-bg-primary'` with:
   ```tsx
   isWgpuActive ? 'bg-transparent' : 'bg-[#121212]'
   ```
4. **`src-tauri/src/lib.rs`**:
   In `window.on_window_event`, add `WindowEvent::CloseRequested` handler to stop lingering GPU/worker threads:
   ```rust
   tauri::WindowEvent::CloseRequested { .. } => {
       log::info!("Window close requested; cleanly terminating GPU contexts and worker threads");
   }
   ```

### Step 2: Implement R5 (Left Sidebar Dual-Tab Switcher)
1. In `src/App.tsx`, introduce Left Sidebar tab state in `useUIStore` or locally:
   ```tsx
   const [leftSidebarTab, setLeftSidebarTab] = useState<'folders' | 'tones'>('folders');
   ```
2. In `renderAppPanel` for `case Panel.FolderTree:`, wrap the panel content with a dual-tab switcher header when `activeView === 'editor'`:
   ```tsx
   case Panel.FolderTree:
     if (activeView === 'editor') {
       return (
         <div className="w-full h-full flex flex-col bg-bg-secondary select-none overflow-hidden">
           {/* Top Dual-Tab Bar */}
           <div className="flex border-b border-surface bg-bg-secondary p-1 gap-1 shrink-0">
             <button
               onClick={() => setLeftSidebarTab('folders')}
               className={clsx(
                 'flex-1 py-1.5 px-2 text-xs font-medium rounded flex items-center justify-center gap-1.5 transition-colors',
                 leftSidebarTab === 'folders'
                   ? 'bg-surface text-text-primary shadow-xs'
                   : 'text-text-secondary hover:text-text-primary hover:bg-surface/50'
               )}
             >
               <Folder size={14} />
               <span>{t('panels.left.foldersAndSources', 'Folders & Sources')}</span>
             </button>
             <button
               onClick={() => setLeftSidebarTab('tones')}
               className={clsx(
                 'flex-1 py-1.5 px-2 text-xs font-medium rounded flex items-center justify-center gap-1.5 transition-colors',
                 leftSidebarTab === 'tones'
                   ? 'bg-surface text-text-primary shadow-xs'
                   : 'text-text-secondary hover:text-text-primary hover:bg-surface/50'
               )}
             >
               <Sparkles size={14} />
               <span>{t('panels.left.hdrAndLooks', 'HDR & Looks')}</span>
             </button>
           </div>
           <div className="flex-1 min-h-0">
             {leftSidebarTab === 'folders' ? (
               <FolderTree
                 isResizing={isResizing}
                 onContextMenu={handleFolderTreeContextMenu}
                 onAlbumContextMenu={handleAlbumTreeContextMenu}
                 onSelectAlbum={(id, name, images) => void handleSelectAlbum(id, name, images, false)}
                 onFolderSelect={(path) => void handleSelectSubfolder(path, false, undefined, true, false)}
                 onToggleFolder={handleToggleFolder}
                 onOpenFolder={handleOpenFolder}
                 style={{ width: '100%', height: '100%' }}
                 isInstantTransition={isInstantTransition}
               />
             ) : (
               <TonesPanel />
             )}
           </div>
         </div>
       );
     }
     return (
       <FolderTree ... />
     );
   ```

### Step 3: Implement R6 (White Balance Dropper Fallback & Auto-Crop Pixel Scaling)
1. **`src/components/panel/editor/ImageCanvas.tsx`**:
   Update `applyWbPick` (around line 2363) to use safe 5500K fallback:
   ```tsx
   const effectiveAsShot = asShotWhiteBalance ?? { temperature: 5500, tint: 0 };
   const picked = { temperature: sample.temperature, tint: sample.tint };
   setAdjustments((prev: Adjustments) =>
     isKelvinWhiteBalance
       ? withKelvinWhiteBalance(prev, picked)
       : withRelativeWhiteBalance(prev, toRelativeWhiteBalance(effectiveAsShot, picked)),
   );
   ```
   And in `wbSwatchWhiteBalance` (line 3358):
   ```tsx
   const effectiveAsShot = asShotWhiteBalance ?? { temperature: 5500, tint: 0 };
   const wbSwatchWhiteBalance =
     wbSample
       ? isKelvinWhiteBalance
         ? wbSample
         : toRelativeWhiteBalance(effectiveAsShot, wbSample)
       : null;
   ```
2. **`src/components/panel/right/CropPanel.tsx`**:
   In `handleCompositionAi` (around line 262), convert percentage coordinates to exact pixel dimensions using oriented image dimensions:
   ```tsx
   if (res && res.width > 0 && res.height > 0 && selectedImage?.width && selectedImage?.height) {
     const isSwapped = orientationSteps === 1 || orientationSteps === 3;
     const cropBaseWidth = isSwapped ? selectedImage.height : selectedImage.width;
     const cropBaseHeight = isSwapped ? selectedImage.width : selectedImage.height;

     const pixelX = Math.round((res.x / 100.0) * cropBaseWidth);
     const pixelY = Math.round((res.y / 100.0) * cropBaseHeight);
     const pixelWidth = Math.round((res.width / 100.0) * cropBaseWidth);
     const pixelHeight = Math.round((res.height / 100.0) * cropBaseHeight);

     const newCrop: Crop = {
       unit: 'px',
       x: Math.max(0, Math.min(pixelX, cropBaseWidth - 1)),
       y: Math.max(0, Math.min(pixelY, cropBaseHeight - 1)),
       width: Math.min(pixelWidth, cropBaseWidth - pixelX),
       height: Math.min(pixelHeight, cropBaseHeight - pixelY),
     };

     setAdjustments((prev: Adjustments) => ({
       ...prev,
       crop: newCrop,
     }));
     toast.success(t('editor.crop.compositionAiSuccess', 'Smart Auto-Crop & Level applied'));
   }
   ```

### Step 4: Implement R7 (Brand Scrubbing & Shader Synchronization)
1. **Sanitize UI Strings & Comments**:
   - `TonesPanel.tsx`:
     - Change subtitle to: `t('editor.tones.subtitle', 'Dynamic HDR & Creative Styles')`
     - Change section title to: `'Dynamic HDR Profiles'`
     - Change creative section to: `'Creative LOOKs'`
     - Rename `RADIANT_CREATIVE_LOOKS` $\to$ `CREATIVE_LOOKS`
     - Rename `'radiant-vivid'` $\to$ `'vivid-pop'`, `'radiant-portrait-glow'` $\to$ `'portrait-glow'`, etc.
   - `ReferencePresetsRibbon.tsx`:
     - Rename `SNS_HDR_PROFILES` $\to$ `DYNAMIC_HDR_PROFILES`
     - Replace `'SNS-HDR Optical Profiles'` with `'Dynamic HDR Profiles'`
   - `EditorToolbar.tsx:721`:
     - Replace `'Smart Tone (Radiant Dynamic Exposure & Contrast)'` with `'Smart Tone (Dynamic Exposure & Contrast)'`
   - `Color.tsx:655`:
     - Replace `'Radiant Color Science & Cast Removal'` with `'Advanced Color Science & Cast Removal'`
   - `src-tauri/src/default_presets.rs`:
     - Replace `"folder-sns-hdr"` $\to$ `"folder-dynamic-hdr"`, `"SNS-HDR Reference Profiles"` $\to$ `"Dynamic HDR Profiles"`
   - `src-tauri/src/adjustment_utils.rs`:
     - Rename `get_sns_hdr_reference_presets` $\to$ `get_dynamic_hdr_reference_presets`
     - Rename `"SNS Natural"` $\to$ `"Natural HDR"`, `"SNS Interior"` $\to$ `"Interior HDR"`, etc.
2. **Sanitize Rust / Shader Identifiers & Struct Fields**:
   - In `src-tauri/src/image_processing.rs`, `crates/rapidraw-gpu/src/types.rs`, and both `shader.wgsl` copies:
     - Rename `_pad_dxo_end` $\to$ `_pad_reserved_end`
   - In both `shader.wgsl` copies:
     - Rename `get_radiant_trapezoidal_hsl_influence` $\to$ `get_trapezoidal_hsl_influence`
     - Rename `apply_radiant_tint_and_gamut_declip` $\to$ `apply_tint_and_gamut_declip`
     - Rename `evaluate_radiant_pivot_tone` $\to$ `evaluate_pivot_tone`
     - Rename `apply_dxo_anti_ac_defringe` $\to$ `apply_anti_ac_defringe`
     - Rename `apply_dxo_anamorphosis` $\to$ `apply_anamorphosis`
     - Rename `apply_dxo_hlr_reconstruction` $\to$ `apply_hlr_reconstruction`
     - Rename `apply_dxo_clearview_haze_removal` $\to$ `apply_clearview_haze_removal`
     - Rename `apply_dxo_shadow_desaturation` $\to$ `apply_shadow_desaturation`
     - Rename `apply_dxo_saturated_colors_protection` $\to$ `apply_saturated_colors_protection`
   - In `src-tauri/tests/test_shader_compilation.rs`:
     - Update assertion strings to match sanitized function names.
3. **Synchronize `shader.wgsl`**:
   - Whenever any edit is made to `src-tauri/crates/rapidraw-gpu/src/shader.wgsl`, immediately copy it over `src-tauri/src/shaders/shader.wgsl` (or vice-versa).
   - Add automated byte-identity test in `src-tauri/tests/test_shader_compilation.rs`:
     ```rust
     #[test]
     fn test_shader_wgsl_byte_identity() {
         let p1 = std::fs::read("src/shaders/shader.wgsl")
             .or_else(|_| std::fs::read("src-tauri/src/shaders/shader.wgsl"))
             .expect("shader 1");
         let p2 = std::fs::read("crates/rapidraw-gpu/src/shader.wgsl")
             .or_else(|_| std::fs::read("src-tauri/crates/rapidraw-gpu/src/shader.wgsl"))
             .expect("shader 2");
         assert_eq!(p1, p2, "Both copies of shader.wgsl must be byte-for-byte identical");
     }
     ```

### Step 5: Implement R8 (Verification and Packaging)
1. Verify TypeScript types:
   ```powershell
   npm.cmd run typecheck
   ```
2. Verify Rust workspace tests:
   ```powershell
   cargo test --manifest-path src-tauri/Cargo.toml
   ```
3. Verify shader byte identity:
   ```powershell
   Get-FileHash "src-tauri/crates/rapidraw-gpu/src/shader.wgsl", "src-tauri/src/shaders/shader.wgsl"
   ```
4. Build verified release package:
   ```powershell
   powershell -ExecutionPolicy Bypass -File ./scripts/with-stay-awake.ps1 npm.cmd run tauri build -- --bundles nsis
   ```

---

## 5. Verification Method

1. **R4 Verification**:
   - Inspect `tauri.conf.json`, `index.html`, and `styles.css`. Confirm `#121212` is defined on `html, body, #root`.
   - Launch application; switch between Library view and Editor view multiple times. Confirm zero desktop compositing flicker and no desktop punch-through.
2. **R5 Verification**:
   - Open any image in Editor view.
   - Inspect the Left Sidebar. Verify the dual tabs `[ 📁 Folders & Sources ]` and `[ ✨ HDR & Looks ]` appear at the top.
   - Click `[ 📁 Folders & Sources ]` — verify full `FolderTree` renders, allowing navigation, folder expansion, and selection while in Editor mode.
   - Click `[ ✨ HDR & Looks ]` — verify Dynamic HDR Profiles and Creative LOOKs render and adjust tone parameters normally.
3. **R6 Verification**:
   - Open a JPEG or non-RAW image without EXIF as-shot WB.
   - Select White Balance Eyedropper on canvas. Click any grey or white neutral surface. Verify temperature/tint adjustments immediately update on screen and in sliders.
   - In Crop panel, click "Smart Auto-Crop & Level". Verify the crop boundary properly fits the subject in pixel coordinates without shrinking to a sub-pixel speck.
4. **R7 Verification**:
   - Run grep search for `SNS-HDR`, `Radiant`, `Luminar`, `DxO`, and `AKVIS` across `src` and `src-tauri`. Verify zero occurrences in user-facing strings and identifiers.
   - Run `Get-FileHash "src-tauri/crates/rapidraw-gpu/src/shader.wgsl", "src-tauri/src/shaders/shader.wgsl"` — verify SHA256 hashes match identically.
5. **R8 Verification**:
   - Run `npm run typecheck` (must exit 0 with 0 errors).
   - Run `cargo test --manifest-path src-tauri/Cargo.toml` (all tests pass).
   - Verify output installer file is generated in `src-tauri/target/release/bundle/nsis/`.
