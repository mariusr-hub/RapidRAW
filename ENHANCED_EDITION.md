# RapidRAW Enhanced: A Plain-Language Guide for Photographers

*For people who love photos and don't want to become software engineers to edit them.*

> ### ⚠️ A Humble Note & Disclaimer from the Creator
> 
> *Hi! A quick and honest confession before you dive in: **this is my very first "vibecoded" project**.*
> 
> *I am not a veteran systems engineer or a lifelong software architect. I built and shaped this enhanced edition out of a genuine passion for photography and as a hands-on learning adventure—working closely alongside AI coding assistants to explore how far computational photography can go.*
> 
> *Because of that, **it is definitely not ideal or bug-free**. You will likely encounter rough edges, things that could be engineered more elegantly, or workflows that need extra polishing. Please treat this version as an experimental, passionate labor of learning rather than a polished corporate release. If you run into quirks or issues, your feedback, patience, and advice are deeply appreciated!*

---

## First, what is RapidRAW?

When a good camera takes a picture, it can save two kinds of file. A JPEG is a finished picture: the camera has already decided on the colours, the brightness and how much detail to keep. A RAW file holds everything the sensor recorded, with no decisions made yet. That means far more room to rescue a dark sky, recover a bright window or fix strange colours from indoor lighting.

To work with RAW files you need a RAW editor. Adobe Lightroom is the best-known one, and it costs a monthly subscription. **RapidRAW** is a free, open-source alternative. It's fast because it uses your computer's graphics card, the same chip that runs video games, to do the heavy work.

The official RapidRAW is already a capable editor. This document is about **our enhanced edition**, which is built on top of it and adds a large set of tools aimed at one goal: **getting from a memory card full of photos to finished, shareable pictures with far less manual work.**

---

## The short version

If you only read one section, read this one.

| | Official RapidRAW (1.6.7) | Our enhanced edition |
|---|---|---|
| Everyday editing (light, colour, sharpness) | Yes | Yes, same foundation |
| HDR (combining light and dark exposures) | Basic | A full HDR studio, including photos taken handheld and with moving subjects |
| Panoramas | Basic stitching | Better stitching, plus HDR panoramas in one step |
| Noise removal (grainy night photos) | Yes | A dedicated workspace with side-by-side comparison and batch processing |
| Picking the best shots from a shoot | Manual | Automatic: sharpness, faces and eyes are checked for you |
| Night sky and star photos | No | Star stacking for astrophotography |
| "Make it look like this photo" | No | Copies the colour mood of any reference image |
| Film Simulations & Retro Styles | Basic LUTs / effects | 193 analog presets (Portra, Tri-X, Velvia, CineStill) + SpektraFilm 3D LUTs |
| Fake background blur (portrait look) | Basic lens blur | Realistic simulated lens blur with selectable aperture shapes |
| Exporting for clients and social media | Standard export | Ready-made packs: print, Instagram, Stories, watermarked proofs, web gallery |
| Shooting with the camera connected by cable | Basic | Remote shutter and live view (Canon EOS) |
| Immich photo-server sync | **Yes** | **No, removed in this edition** |
| AI face-part and text-prompt masks | **Yes** | **No, removed in this edition** |
| AI speed-up on Mac | **Yes** | **No** (the Windows speed-up was added) |

The biggest leaps are **HDR**, **automatic culling** (choosing the keepers) and **delivery** (sending finished work to clients and social media). Those are the areas where the enhanced edition does the most work for you.

---

## 1. HDR: when one photo can't capture what your eyes saw

**The problem.** You stand in a church. The stained glass glows and the wooden pews sit in deep shadow. Your eyes see both. Your camera can't: either the windows turn pure white or the pews turn pure black.

**The usual fix** is to take several photos at different brightness levels (photographers call this *bracketing*) and blend them into one. In the official version this works best when everything is perfectly still and the camera sits on a tripod.

**What our edition adds:**

- **Handheld HDR that copes with movement.** If you held the camera by hand, or a cyclist rode through the frame, or leaves moved in the wind, the software lines the photos up precisely and avoids "ghosts", the half-transparent doubles of anything that moved.
- **It can tell separate moments apart.** If you shot one bracket at 10:00 and another at 10:05, it won't accidentally merge them into one confused picture.
- **Natural-looking results.** HDR has a reputation for looking fake: glowing halos around trees and grey, washed-out blacks. Our edition includes an automatic quality check that scores each result for halos, flat blacks and harsh highlights, then corrects them. The aim is a photo that looks like what you saw, not like a painting.
- **A pre-flight check.** Before merging, it inspects your frames and warns you if something will cause trouble, such as large movement between shots or a frame that is completely blown out.
- **Scene presets** give you a sensible starting point so you aren't facing a wall of sliders.

**Why it matters to you:** interiors, sunsets, landscapes with bright skies and city scenes at dusk can all be shot handheld and still come out clean.

---

## 2. Panoramas, and HDR panoramas in one step

**Panoramas** are several photos taken while turning on the spot, joined into one wide image.

Our edition improves the joining in three ways:

- **Seams are placed intelligently**, so the join lines run through areas where they won't be noticed instead of cutting through a face or a building.
- **Brightness is evened out** between frames, so you don't get a visible step in the sky where one photo was slightly darker than the next.
- **Different projection styles** (flat, cylindrical and spherical) suit different scenes, from a city street to a full 360° view.

The headline feature is the **one-step HDR panorama.** Say you shot five angles with three brightness levels each: fifteen photos. Before, you would have had to build five HDR images, save them, and then stitch those together. Now you give the software all fifteen and get one finished wide, high-detail image back.

For difficult cases, the edition can also call on **Hugin**, a long-established free panorama toolkit, for extra precision.

---

## 3. Noise removal: rescuing night and indoor photos

**The problem.** In low light, photos get grainy, with coloured speckles in the shadows. Photographers call this *noise*.

The official version already removes noise. Ours gives it a **dedicated workspace**:

- **Before-and-after views.** You can drag a divider across the image, put the original and the cleaned version side by side, or zoom to 100% to inspect fine detail.
- **Snapshot comparison.** You can save up to three versions (A, B and C) with different settings and flip between them to choose your favourite.
- **It knows your camera.** Each camera model produces its own kind of grain, and the software uses camera-specific profiles to clean it more accurately.
- **It recommends a method.** Several cleaning methods are available, and the software suggests the one that suits your photo best.
- **Batch mode.** You can clean an entire evening's shoot in one go.

---

## 4. Automatic culling: finding the keepers for you

After a wedding, a sports event or a family holiday you might come home with **1,500 photos**. Many are near-duplicates. Some are blurry. In some, someone blinked. Going through them one by one can take longer than the editing itself.

Our edition adds two tools here:

- **Speed Culler** goes through a whole shoot and checks sharpness, detects faces and eyes, and shows close-ups so you can confirm focus without zooming yourself. It then suggests which photos to keep and which to drop.
- **Hero Curator** looks at a burst or a series of similar shots and picks the single best one (the sharpest and strongest frame), then gives it a five-star rating automatically.

**Why it matters to you:** this removes the most tedious hour of your workflow. You still make the final call, but you're reviewing suggestions instead of starting from zero.

---

## 5. One-click improvements that understand what's in the photo

- **Auto Polish** recognises what kind of scene you shot, such as a landscape, a portrait or a night scene, and applies suitable adjustments for that scene instead of one generic "auto" setting for everything. It also works across a whole shoot at once (**Batch Polish**).
- **Sky Sculptor** finds the sky by itself and lets you apply looks such as deep blue, warm sunset or dramatic storm clouds **to the sky only**, without tinting the people or buildings below.
- **Steal the Look (Colour Matcher).** Show it any picture you admire, like a film still, a vintage photograph or a favourite photographer's work, and it carries that colour mood over to your photo.
- **Film-like colour.** A new colour engine gives highlights a soft, gentle roll-off, the way classic film behaved, instead of a harsh jump to white. Shadows stay clean instead of muddy.

---

## 6. Repair and finishing tools

- **Automatic horizon straightening.** It detects a tilted horizon and levels it.
- **Smart cropping.** It finds the main subject and suggests a well-balanced crop. When you rotate a photo, it automatically trims the empty corners that rotation creates.
- **Sensor dust removal.** Specks of dust on the camera sensor show up as small grey blobs, especially in skies. The software finds them, double-checks them against your other photos so it doesn't mistake a bird for dust, and removes them.
- **Lens correction workspace** for fixing bent lines and dark corners caused by the lens.
- **Transform workspace** for straightening buildings and correcting perspective.
- **Film negative conversion.** If you scan old negatives, a guided tool turns them into normal positive colour photos.
- **Upscaling (Super Resolution)** enlarges a photo for big prints while keeping edges sharp.

---

## 7. Creative tools

- **Bokeh simulator.** "Bokeh" is the soft, blurred background behind a sharp subject in portraits. It normally requires an expensive lens. Our edition recognises the subject and blurs the background realistically. You can pick the shape of the out-of-focus light spots (round, nine-blade like a real lens, or the stretched oval "anamorphic" style used in cinema).
- **Astrophotography star stacking.** Night-sky shots are usually grainy. If you take many photos of the stars and give them to the stacker, it aligns the stars, removes hot pixels and sky-glow gradients, and combines everything into one clean, detailed image of the night sky.
- **Focus stacking.** For close-ups such as flowers and insects, where only a thin slice is ever in focus, you can combine several photos focused at different depths into one image that is sharp from front to back.
- **Four-way variant view.** See four versions of an edit side by side and pick the best.

---

## 8. Analog Film Emulation: 190+ Vintage & Cinema Stocks + SpektraFilm LUTs

**What changed?**
In the official version, creative effects were limited to general color sliders, manual grading, and generic LUT files. Our enhanced edition ships with a massive analog heritage library built directly into the engine:

- **193 Built-in Film Simulation Presets**: Faithfully mimics classic analog films from Kodak, Fujifilm, Ilford, and cinematic motion-picture emulsions:
  - **Portrait & Wedding Classics**: Kodak Portra 160, 400NC, 800; Fuji Pro 400H, Astia 100F.
  - **Street & Everyday Retro**: Kodak Gold 200, ColorPlus 200, UltraMax 400, Kodak Ektar 100.
  - **Fujifilm Camera Simulations**: Classic Chrome, Pro Neg Hi, Velvia 50, Provia 100F, ACROS black-and-white.
  - **True Cinematic Tungsten & Daylight**: CineStill 800T (with night tungsten halation), Kodak Vision3 250D, Vision3 500T.
  - **Heritage European & Rare Stocks**: Ilford HP5+ 400, Fomapan 100, Svema Foto 100, Konica Impresa 50, Original Wolfen NC500, SantaColor, Candido, and LomoChrome Purple.
- **Bundled SpektraFilm 3D LUTs**: Ships with licensed calibrated 3D Cube LUTs (`K-Portra 400`, `F-Pro 400H`, `K-Ektar 100`, `K-Gold 200`, `K-Vision 250D`, `K-Vision 500T`) ready to apply with adjustable strength.
- **Filmic Tone Response & Highlight Desaturation**: Film rolls never hit an abrupt, chalky white wall when exposed to bright sun. Our edition includes filmic highlight roll-off and shadow chromatic neutralization, so intense highlights softly feather into pure light just like analog negative film.

---

## 9. Exporting and delivery: getting photos to the people who need them

This is the second big leap, especially if you photograph for clients or post a lot online.

- **Client Delivery Pack.** One click produces five ready-to-send folders:
  1. Full-resolution master for printing
  2. Instagram post size (4:5)
  3. Instagram Stories and Reels size (9:16)
  4. Watermarked web proofs for client approval
  5. A small, fast-loading web gallery version
- **Advanced export** runs several photos in parallel and supports professional colour spaces for printing.
- **Text watermarks** can fill in your photo details automatically (camera, lens, settings, year).
- **Privacy Guard** strips location data from photos before you share them, so strangers can't see where you live.
- **Stock photography preparation.** If you sell photos on stock websites, a dedicated tool prepares them to agency standards, and a checker flags visible logos or brand text that agencies often reject.
- **Professional file output.** You can save finished HDR or merged results as high-precision DNG files that other professional editors (Lightroom, Capture One) can open.

---

## 10. Shooting with your camera connected

**Tethering** means shooting while the camera is connected to your computer by cable. Each photo appears on the big screen straight away, which is useful in a studio or for product photos. Our edition adds remote shutter release, a live view on the screen and automatic import. *Note: this currently targets Canon EOS cameras.*

---

## 11. A gentler experience on ordinary laptops

Heavy photo processing can make a thin laptop run hot and noisy. Our edition includes a **"stability" safeguard** that leaves some of the processor free so your computer stays responsive and doesn't overheat during long jobs. It also stops the computer from going to sleep in the middle of a long batch.

---

# Part Two: The Detailed Comparison

The first part of this guide covered what the changes mean in everyday use. This part sets out the comparison in more detail, area by area, for readers who want to see exactly what changed. Each table compares the official release (version 1.6.7) with our enhanced edition.

How to read the "Verdict" column:

- **New**: does not exist in the official version
- **Upgraded**: exists in both, clearly more capable in ours
- **Same**: no meaningful difference
- **Reduced** / **Removed**: the official version is better here, or ours doesn't have it

---

## A. File-Set Differences: how much was actually changed

Software is made of files, a bit like chapters in a book. Comparing which "chapters" were added, rewritten or taken out gives an honest measure of how far apart two versions are.

### Overall size of the changes

| Measure | Result |
|---|---|
| Files changed in the app's program code | **217** |
| Lines of code added | about **69,600** |
| Lines of code removed or rewritten | about **31,500** |
| New processing modules (the "engine room") | **38** |
| New screens, panels and helper components | **22** |
| Official modules removed | **18** (12 of them belong to the Immich integration) |
| Official screens and components removed | **15** (8 of them belong to the Immich integration) |

In plain terms, the enhanced edition is a major extension of the official app, not a small patch. Roughly one line in three of the official engine code has been touched.

### New processing modules (engine room)

| Area | New modules | What they do |
|---|---|---|
| HDR | HDR Fusion, Handheld HDR, HDR Panorama, HDR Presets, Deghosting (rewritten) | Merge light and dark exposures, including handheld shots with movement |
| Panorama | Camera Model, Seam Finder (graph cut), Brightness Matching (photometric), Hugin bridge | Cleaner joins, even brightness, optional professional toolkit |
| Noise | Denoising (new engine, about 3,800 lines) | Camera-aware noise cleaning, previews, batch mode |
| Culling | Speed Culler, Hero Curator | Automatic sharpness, face and eye checks; best-frame pick |
| Auto-improve | Semantic Auto Polish, Sky Sculptor, Colour Matcher, Filmic Colour Science, Auto-Tune Loop | Scene-aware one-click edits, sky-only looks, "steal the look", film-like tones |
| Quality control | Photographic Critic, Quality Shield | Scores each result for halos, flat blacks and harsh highlights, and blocks bad output |
| Repair | Defect Repair, Super Resolution | Horizon detection, smart crop, dust removal, upscaling |
| Creative | Bokeh Simulator, Astro Stacking (plus a GPU star-stacking shader) | Background blur, night-sky stacking |
| Delivery | Batch Export Engine, Client Delivery, Stock Prep, Agency Uploader, Compliance Inspector, DNG Encoder | Export packs, watermarks, privacy, stock-agency preparation, professional file output |
| Hardware | Tethering, Stability, Sleep Lock, Fast Resizer | Camera control, overheating protection, stops sleep during long jobs, faster resizing |

### New screens (what you see)

| Type | New items |
|---|---|
| Workspaces (pop-up windows) | Advanced Export, Batch Polish, Bokeh, Client Delivery, Colour Matcher, Denoise, Drizzle (star detail), HDR Panorama Pre-flight, Hero Curator, Lens Correction, Negative Conversion, Speed Culler, Tethering, Transform |
| Editor views | Split-compare overlay, four-way variant matrix |
| Drop zones | Focus Stack, Night Sky, Stock Prep |
| Safety | Error boundary (if one panel fails, the rest of the app keeps working) |

### Removed from the official version

| Removed item | What it means for you |
|---|---|
| Immich integration (20 files) | No connection to an Immich photo server |
| Face-part masking (and its illustration guide) | You can't select individual face parts such as eyes or lips with AI |
| Background AI job queue | Long AI tasks are managed differently, and the official progress queue is gone |
| Memory budget settings | The official settings for export concurrency and RAM limits are not available |
| Data-management settings screen | The related settings page is gone |
| Small UI items (filter-exit notice, toolbar menu, connected waveform) | Minor interface differences |

---

## B. Feature-Level Improvements at a Glance

| Feature | Official 1.6.7 | Enhanced edition | Verdict |
|---|---|---|---|
| HDR merging | Bracket merging | Full HDR studio: handheld alignment, ghost removal, pre-flight check, quality scoring, presets | **Upgraded** |
| HDR panorama | Not available as one step | Merge and stitch bracketed panoramas in one go | **New** |
| Panorama stitching | Seamless stitcher | Smarter seams, brightness matching, three projection types, optional Hugin | **Upgraded** |
| Noise reduction | AI denoise, BM3D, manual luma/colour sliders | Dedicated workspace: camera-specific profiles, method recommendation, A/B/C snapshots, batch mode | **Upgraded** |
| Culling | Culling view with ratings and flags | Adds automatic Speed Culler and Hero Curator | **Upgraded** |
| One-click scene-aware editing | Auto adjust (subtle, normal, strong) | Scene recognition with tailored adjustments, single or batch | **Upgraded** |
| Sky-only looks | Sky mask (manual adjustments) | Sky Sculptor presets with no bleed onto the foreground | **New** |
| Copy colours from a reference photo | No | Colour Matcher ("steal the look") | **New** |
| Background blur | Depth-based lens blur | Adds a bokeh simulator with aperture shapes | **Upgraded** |
| Astrophotography stacking | No | Star alignment, hot-pixel and gradient removal, Drizzle | **New** |
| Focus stacking | Yes | Yes, with a drag-and-drop zone | **Same / slightly upgraded** |
| Dust removal | Manual heal/clone | Automatic detection, cross-checked across photos | **Upgraded** |
| Horizon and crop | Auto straighten, manual crop | Adds horizon detection, subject-aware crop and auto-trim after rotation | **Upgraded** |
| Upscaling | No | Super Resolution | **New** |
| Client export packs | No | Five ready-made delivery folders | **New** |
| Watermarks and privacy | Basic export | Text watermarks with photo details, GPS removal | **New** |
| Stock photography preparation | No | Stock Prep plus logo and text checker | **New** |
| Tethered shooting | Basic tethering panel | Remote shutter, live view, auto-import (Canon EOS) | **Upgraded** |
| AI face-part masks | Yes | No | **Removed** |
| AI text-prompt masks ("select the red car") | Yes | No | **Removed** |
| Immich sync | Yes | No | **Removed** |

---

## C. Core Editing Engine

The "engine" is the invisible part that turns the RAW data into the picture on your screen. Both versions share the same foundation (fast processing on the graphics card), but large parts of it were rewritten in our edition.

| Engine area | Official 1.6.7 | Enhanced edition | What you notice | Verdict |
|---|---|---|---|---|
| Main image processor (the graphics-card "shader") | Mature, compact | About 1,700 lines reworked, with new colour, tone and lens routines | More natural colours and smoother highlights | **Upgraded** |
| Highlight recovery | Separate module | Built into the main processor, rebuilding colour in bright areas | Bright skies and lamps keep their colour instead of turning flat white | **Upgraded** |
| Dehaze | Standard | New version with a refined haze estimate | Clearer distant landscapes with fewer dark halos | **Upgraded** |
| Tone curves | Standard RGB curves | Adds a mode that preserves colour while changing contrast | Contrast changes don't shift colours | **Upgraded** |
| Colour fringing (purple and green edges) | Chromatic-aberration sliders | Adds an optical "defringe" step | Cleaner edges on branches against the sky | **Upgraded** |
| Lens distortion | Profile-based correction | More precise correction model plus a dedicated workspace | Straighter lines with wide-angle lenses | **Upgraded** |
| Tone mapping | Standard options | Adds an OkLab mode (a colour model closer to how eyes perceive colour) and a film-like highlight roll-off | Gentle highlight transitions, like film | **Upgraded** |
| Built-in noise reduction in the main processor | Advanced GPU noise filter | Replaced by the separate denoise engine and workspace | Noise removal is a deliberate step with previews rather than a live slider effect | **Different** |
| RAW decoding and loading | Standard | About 1,000 lines changed in RAW and image loading | Broader handling of difficult files | **Upgraded** |
| Output quality check | None | Automatic quality gate (Photographic Critic and Quality Shield) | Fewer bad merges reach you | **New** |

---

## D. Adjustments and Creative Controls

These are the sliders and controls you use while editing.

### New sliders and controls

| New control | What it does in plain words |
|---|---|
| **Clear View** | Cuts through haze and dullness for a crisper look |
| **Micro-Contrast** | Brings out fine texture (bark, fabric, stone) without harsh "HDR" grit |
| **Contrast Cleaner** | Keeps contrast changes clean, avoiding muddy shadows |
| **Skin Shield** | Protects skin tones from going orange or grey when you push colours (on by default) |
| **Skin de-flash** | Softens the shiny hotspots that a camera flash leaves on faces |
| **Foliage Enhancer** | Makes greenery look lush and natural instead of neon |
| **Orton Spread** | A dreamy, glowing landscape look, used for decades by landscape photographers |
| **Lights group** (brightness, contrast, details, temperature, tint, saturation, vibrance) | Adjusts only the bright parts of the image, with its own colour and detail controls |
| **Supercontrast** (shadows, midtones, highlights, pivots) | Separate contrast control for dark, middle and bright tones |
| **Corner Sharpness Boost** | Sharpens the soft corners that many lenses produce |
| **Bokeh Mask Protection** | Keeps the subject sharp when background blur is applied |
| **Smart Strength, Enhance & Per-Pixel Tone, Rich Colour & Cast Removal, Skin & Portrait Protection** | Grouped "smart" controls in the Basic panel for quick, safe improvements |
| **Kelvin mode and an improved white-balance picker** | Set colour temperature in the same units photographers use (degrees Kelvin) |

### Side-by-side control comparison

| Control group | Official 1.6.7 | Enhanced edition | Verdict |
|---|---|---|---|
| Basic light (exposure, contrast, highlights, shadows, whites, blacks) | Yes | Yes, plus Smart Strength and protection groups | **Upgraded** |
| White balance | Temperature and tint | Adds Kelvin mode and an improved picker | **Upgraded** |
| Colour mixer (HSL) | Yes | Smoother colour-range transitions | **Upgraded** |
| Contrast tools | Contrast, curves | Adds Supercontrast, Micro-Contrast, Contrast Cleaner | **Upgraded** |
| Portrait tools | Masks, retouch, liquify | Adds Skin Shield and skin de-flash; AI face-part masks removed | **Mixed** |
| Landscape tools | Dehaze, clarity, structure | Adds Clear View, Foliage Enhancer, Orton glow | **Upgraded** |
| Effects (vignette, grain, glow, halation, flares, LUTs, film looks) | Yes | Yes, plus 193 classic film presets & SpektraFilm 3D LUTs | **Upgraded** |
| Spatial effects (lens blur, relight, fog) | Yes | Yes, plus the bokeh simulator | **Upgraded** |
| Masks | AI subject, sky, foreground, depth, text prompt, face parts, colour and luminance | AI subject, sky, foreground, depth, colour and luminance | **Reduced** |
| Built-in presets | Standard set | **193 Analog & Cinema film presets** (Portra, Fuji, Ilford, CineStill, Kodak, SNS-HDR reference profiles) | **Massively Upgraded** |

---

## E. UI/UX Improvements: how it feels to use

| Area | Official 1.6.7 | Enhanced edition | Benefit | Verdict |
|---|---|---|---|---|
| Complex tasks | Mostly side panels and short dialogs | 14 dedicated workspaces (Denoise, HDR pre-flight, Transform, Lens Correction, and more) | Each big task has its own uncluttered screen | **Upgraded** |
| Before and after | Full, split and side-by-side compare | Adds a draggable split overlay, A/B/C snapshots and a four-way variant view | Easier to choose between versions | **Upgraded** |
| Starting a merge or stack | Select files, then open the tool | Drag-and-drop zones for focus stack, night sky and stock prep | Fewer clicks | **Upgraded** |
| Warnings before long jobs | None | HDR panorama pre-flight check | Problems are caught before you wait | **New** |
| Crash containment | A panel error can affect the app | Error boundary isolates the failure | The app keeps running | **New** |
| Toolbar | Toolbar menu | Simplified toolbar (menu removed) | Cleaner, though some quick options moved | **Different** |
| Languages | 15 languages, fully translated | Same 15 languages, but some new screens are English-only and German, French and Japanese are about 5–10% less complete | Non-English users may see some English text | **Reduced** |

---

## F. New Workflows to Save You Time

Each row compares how a common job is done in each version.

| Job | Official 1.6.7 | Enhanced edition | Typical time saving* |
|---|---|---|---|
| Sort 1,000 shots from an event | Rate each photo by hand in the culling view | Speed Culler checks focus, faces and eyes; Hero Curator picks the best frame of each burst | Large: hours become a review of suggestions |
| Handheld HDR of an interior | Works best on a tripod; movement causes ghosts | Handheld alignment and ghost removal | Fewer re-shoots and manual fixes |
| HDR panorama (for example 15 photos) | Merge each angle, export, then stitch | One step | Several manual rounds avoided |
| Clean a whole night shoot | Denoise per image | Batch denoise with camera profiles | Large for big shoots |
| Edit a full shoot with a consistent look | Copy and paste settings, then fine-tune each photo | Batch Polish adapts to each scene | Moderate |
| Deliver to a client | Export several times at different sizes | One-click Client Delivery Pack (five formats) | Large for working photographers |
| Prepare stock photos | Manual checking and exporting | Stock Prep plus logo and text checker | Moderate to large |
| Match a look from a reference photo | Manual colour grading | Colour Matcher | Large for beginners |
| Remove sensor dust across a series | Heal each spot by hand | Automatic detection, cross-checked across photos | Large for landscape and sky photos |

\* These are expected savings based on how the workflows are built. We have not measured them as timed benchmarks.

---

## G. Performance and Stability Improvements

This section lists the drawbacks alongside the gains.

| Area | Official 1.6.7 | Enhanced edition | Verdict |
|---|---|---|---|
| AI acceleration on Windows | GPU acceleration | Adds DirectML support (the Windows system for running AI on any graphics card) | **Upgraded on Windows** |
| AI acceleration on Mac | CoreML (Apple's AI acceleration) | CoreML support not present | **Reduced on Mac** |
| Image resizing (previews, exports) | Standard | Dedicated fast-resize library | **Upgraded** |
| Laptop heat | Uses all processor cores | Caps heavy jobs to protect the machine and keep it responsive | **Upgraded on laptops**; may be **slower on powerful desktops** |
| Long batch jobs | The computer may go to sleep | Sleep Lock keeps it awake until the job finishes | **New** |
| Memory controls | Settings for export concurrency and free-RAM limits | Not available | **Reduced** |
| Background job queue | Unified AI job queue with progress tracking | Removed; tasks are handled inside each workspace | **Reduced** |
| Panel crashes | Can disrupt the app | Contained by the error boundary | **Upgraded** |
| Output quality safety net | None | Automatic quality gate on merges and stitches | **New** |
| Build optimisation | Maximum release optimisation | Faster-to-build settings (lighter optimisation) | **Possibly slightly slower** in raw speed until the release settings are restored |
| Automated quality tests | No unit tests in the engine source | About 200 automated engine tests, plus visual-quality benchmark suites | **Upgraded** |

---

## H. Documentation and Support

| Resource | Official 1.6.7 | Enhanced edition | Verdict |
|---|---|---|---|
| Project description (README) | Current and complete for the official features | Not yet updated: it still describes older features and doesn't list the new workspaces | **Needs work** |
| In-app text and tooltips | Fully translated in 15 languages | English complete; some new features are English-only and some translations are incomplete | **Reduced** |
| Community and bug reports | Official GitHub, website, Discord community | Not covered by the official community, since this is an independent edition | **Reduced** |
| Developer tools | Benchmark scripts | Adds command-line render and analysis tools, plus test suites for HDR, panorama and output formats | **Upgraded** |
| Website and user guide | getrapidraw.com docs | No dedicated guide yet (this document is the first step) | **Needs work** |

---

## What the official version has that ours does not

Before switching, you should know:

- **Immich integration.** The official version can connect to *Immich*, a self-hosted photo library server, and sync your edits and ratings with it. **Our edition has removed this feature.** If you rely on Immich, stay with the official version for now.
- **Advanced AI masks.** The official version can select individual face parts (eyes, lips, skin) and can select objects from a typed description ("the red car"). **Neither is in our edition.**
- **Mac AI acceleration (CoreML).** On Mac, AI features in the official version run faster.
- **Memory and job-queue controls.** The official version has a background job queue and RAM and concurrency settings that ours does not have.
- **Complete translations and official support.** The official version is fully translated and backed by its community.

---

## Who should try the enhanced edition?

**It's a strong fit if you:**
- Shoot landscapes, interiors, travel or cityscapes and want natural-looking HDR and panoramas
- Come home from events with hundreds of photos and dread sorting them
- Photograph for clients and spend time resizing and exporting for different uses
- Want to try astrophotography or the blurred-background portrait look without new gear
- Want a free, capable alternative to subscription software

**Stay with the official version if you:**
- Depend on the Immich photo server
- Retouch portraits using AI selection of eyes, lips or skin, or select objects by typing a description
- Edit on a Mac and want the fastest AI features
- Prefer the app fully in a language other than English
- Want only the officially supported, community-released build

---

## The bottom line

Official RapidRAW gives you a fast, free editor. Our enhanced edition is designed to take over the slow parts around the editing: sorting the shoot, merging difficult exposures, joining panoramas, cleaning grain, and packaging everything for clients and social media. You keep full creative control and spend less of your evening on the repetitive steps.

---

## Part Three: Getting Started (A Quick 5-Step Guide for Beginners)

If you have never used an advanced RAW photo editor before, don't worry about mastering all 14 workspaces at once. Here is the easiest path to see the difference on your own photos immediately:

1. **Drop in a folder of photos**: Open RapidRAW Enhanced and browse to a folder from your camera card. The thumbnails will populate rapidly.
2. **Sort the keepers with Speed Culler**: If you took lots of similar shots, open the **Speed Culler** modal. Let it zoom in to inspect focus on eyes and subjects automatically, hit the suggested keep/reject shortcuts, and narrow your shoot down to only the sharpest memories.
3. **Try One-Click "Auto Polish"**: Open your favorite photo in the editor and click **Auto Polish**. Instead of slapping a flat generic filter, it reads the scene (landscape, portrait, or indoor) and gently sets a balanced baseline with protected skin tones and natural highlights.
4. **Fix Tricky Lighting**: 
   - Did you take a high-contrast sunset or bright window shot? Use **HDR Fusion**.
   - Did you shoot indoors at night with grain? Open the **Denoise Modal**, pick the recommended camera profile, and use the split slider to find the sweet spot between clean shadows and crisp details.
5. **Export Ready-to-Use Files**: When you're ready to share, open **Client Delivery** or **Advanced Export**. Pick your destination (full-resolution master for family archives or print, or social sizes for phones) and export everything in one clean step with location privacy turned on.

---

# Part Four: Technical Deep Dive & Developer Architecture

> **For engineers, technical contributors, and GitHub release notes.**  
> **Note from the author:** *This is my first vibecoded codebase, written in collaboration with AI as a deep dive into computational photography and Rust. If you spot areas where patterns or memory abstractions can be improved, issues, PRs, and refactoring tips are warmly welcomed!*  
> **Baseline:** Upstream `CyberTimon/RapidRAW` @ `v1.6.7` (commit `a4f2cf5`)  
> **Repository Delta:** +69,620 / -31,497 lines across 217 core files.

---

### 1. Architectural & Subsystem Reorganization

#### Removed Subsystems (Intentional Decoupling)
* **Immich Subsystem Purged:** Complete removal of `src-tauri/src/immich/*` (12 modules) and `src/components/immich/*` (8 TSX/TS files) to decouple cloud daemon background sync overhead from raw local processing. Keyring native stores (`windows-native-keyring-store`, `apple-native-keyring-store`, `zbus-secret-service-keyring-store`) were removed from `Cargo.toml`.
* **Legacy AI Queue Scrapped:** Deleted `src-tauri/src/ai_jobs.rs` and replaced monolithic queue with per-modal async streaming channels (`tokio::sync::mpsc`).
* **Face Part Masking:** Deprecated `src-tauri/src/face_masking.rs` in favor of full-body semantic segmentation and localized inpainting.

#### New Computational Rust Modules (`src-tauri/src/`)
* **`hdr_fusion.rs` & `hdr_handheld.rs` (~3,500 LOC):** Replaces naive EV stacking with Mertens-Kautz-Van Reeth Laplacian pyramid exposure fusion. Handheld alignment uses multi-scale Huber-IRLS iterative projective homography (`align_image_stack` logic) with motion-adaptive photometric deghosting.
* **`hdr_panorama.rs` (~912 LOC):** Direct 32-bit linear floating-point tile stitcher bypassing intermediate 16-bit TIFF serialization.
* **`panorama_utils/` Subsystem:**
  * `graph_cut.rs`: Min-cut/max-flow seam optimization to bypass moving subjects across frame boundaries.
  * `photometric.rs`: Vignette-aware radial gradient balancing and exposure-ramp equalization.
  * `camera_model.rs`: Full radial $k_1, k_2, k_3$ lens de-distortion solver.
* **`hugin_engine.rs` (~725 LOC):** Sub-process bridge orchestrating external `cpfind`, `linefind`, `autooptimiser`, `nona`, and `enblend`.
* **`denoising.rs` (~3,820 LOC):** Replaces legacy `bm3d.rs` with sensor-calibrated noise profile estimation (`SensorNoiseProfile`) combined with multi-threaded tiled spatial filtering.
* **`astro_stacking.rs` (~2,280 LOC):** Star-centroid triangle-matching registration, sigma-clipped kappa stacking, hot-pixel suppression, and sky background polynomial gradient flattening.
* **`quality_shield.rs` & `photographic_critic.rs`:** Invariant validation layer. Computes Laplacian edge overshoot metrics (anti-halo protection), Ansel Adams Zone 0 black-floor anchoring, and gamut clipping audits before dispatching rendered buffers to the front-end.
* **`dng_encoder.rs` (~668 LOC):** Custom pure-Rust 32-bit IEEE-754 floating-point Linear DNG (TIFF 6.0 / DNG 1.4) writer. Preserves full dynamic range for downstream workflows in Adobe Camera Raw / Capture One.

---

### 2. GPU Shaders & Color Science (`src-tauri/src/shaders/shader.wgsl`)

The central WebGPU/wgpu compute shader was significantly overhauled (~1,700 lines changed):

* **Colorimetry Pipeline:**
  * Transitioned intermediate tone mapping from naive sRGB/Rec.709 to **Oklab** perceptual color space (`srgb_norm_to_oklab`).
  * Added `rgb_to_iqbqr` and `iqbqr_to_rgb` transforms.
* **Filmic Highlight Rolloff & Gamut Protection:**
  * New `apply_subtractive_tint_and_gamut_declip`: Reconstructs highlight chroma near sensor saturation limits, avoiding cyan/magenta highlight clipping cliffs.
  * `apply_deep_shadow_chroma_damping`: Suppresses chroma noise spikes in the bottom 3% luminance floor.
* **Tone & Micro-Contrast Functions:**
  * `evaluate_dynamic_pivot_tone` & `apply_supercontrast_pivots`: Per-zone tone mapping curve.
  * `apply_skin_deflash_specular_rolloff`: Non-linear specular rolloff on skin mask pixels.
  * `distort_coord_6th_order`: Newton-Raphson 6th-order polynomial coordinate distortion solver for real-time lens aberration correction.

---

### 3. Cargo Dependencies & Hardware Acceleration

```toml
# Changes in src-tauri/Cargo.toml

[dependencies]
# ONNX Runtime DirectML feature added for hardware-agnostic GPU inference on Windows:
ort = { version = "=2.0.0-rc.10", features = ["ndarray", "load-dynamic", "directml"] }

# High-performance parallel image resampling:
fast_image_resize = "5.1"

# Internal DSP routines:
rapidraw-dsp = { path = "crates/rapidraw-dsp" }

# Removed Platform Keyrings (Immich removal):
# - keyring-core
# - windows-native-keyring-store
# - apple-native-keyring-store
# - zbus-secret-service-keyring-store
```

#### Thermal & Execution Throttle (`stability.rs`)
* Hard-caps CPU thread pools to **4 physical worker threads** (`ECO_POOL` via `rayon`) on mobile platforms (e.g. ThinkPad L13 envelope) to maintain CPU package temperatures strictly `< 72°C` while leaving OS scheduler headroom.

---

### 4. IPC & Front-End Payload Extensions

#### Extended State & Schema (`src/utils/defaultAdjustments.json`)
New persistent parameters recognized by the pipeline:
```json
{
  "contrastCleaner": 1.0,
  "skinShield": 0.90,
  "foliageEnhancer": 0.0,
  "ortonSpread": 0.0,
  "supercontrastShadows": 0.0,
  "supercontrastMidtones": 0.0,
  "supercontrastHighlights": 0.0,
  "supercontrastPivots": 0.0,
  "cornerSharpnessBoost": 0.0,
  "bokehMaskProtection": 0.0,
  "microContrast": 0.0,
  "clearView": 0.0,
  "lightsBrightness": 0.0,
  "lightsContrast": 0.0,
  "lightsDetails": 0.0
}
```

#### New Tauri IPC Commands Exposed
* `analyze_image_noise_profile(path: String) -> SensorNoiseProfile`
* `preview_denoised_roi(path: String, roi: Rect, method: String) -> ImageData`
* `analyze_and_polish_active_image(intensity: Option<f32>) -> AdjustmentsPayload`
* `batch_polish_photoshoot(paths: Vec<String>) -> TaskStatus`
* `dispatch_to_stock_agencies(config: AgencyUploadConfig) -> Result<(), String>`

---

### 5. CLI Binaries & Verification Test Suites

Two standalone CLI tools were introduced for automated headless profiling:
* **`render_all`** (`src-tauri/src/bin/render_all.rs`): Headless batch compiler that processes RAW directories against test presets.
* **`find_clusters`** (`src-tauri/src/bin/find_clusters.rs`): Burst-detection utility to cluster bursts by timestamp and focal length.

#### Verification & Test Harness
Run the end-to-end mathematical verification suites:
```bash
# Verify HDR alignment, Laplacian weights, and ghost rejection:
cargo test --test test_hdr_benchmark --manifest-path src-tauri/Cargo.toml

# Test Hugin command-line orchestrator integration:
cargo test --test test_hugin_integration --manifest-path src-tauri/Cargo.toml

# Test DNG 32-bit linear float encoder compatibility:
cargo test --test test_save_formats --manifest-path src-tauri/Cargo.toml

# Shader WGSL compilation test:
cargo test --test test_shader_compilation --manifest-path src-tauri/Cargo.toml
```


