import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { Invokes } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from '../utils/adjustments';

export function useImageLoader(cachedEditStateRef: React.RefObject<any>) {
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const histogram = useEditorStore((s) => s.histogram);
  const waveform = useEditorStore((s) => s.waveform);
  const finalPreviewUrl = useEditorStore((s) => s.finalPreviewUrl);
  const uncroppedAdjustedPreviewUrl = useEditorStore((s) => s.uncroppedAdjustedPreviewUrl);
  const originalSize = useEditorStore((s) => s.originalSize);
  const previewSize = useEditorStore((s) => s.previewSize);
  const hasRenderedFirstFrame = useEditorStore((s) => s.hasRenderedFirstFrame);

  const setEditor = useEditorStore((s) => s.setEditor);
  const resetHistory = useEditorStore((s) => s.resetHistory);
  const setLibrary = useLibraryStore((s) => s.setLibrary);
  const appSettings = useSettingsStore((s) => s.appSettings);
  const osPlatform = useSettingsStore((s) => s.osPlatform);

  const isWgpuActive =
    osPlatform !== 'windows' &&
    appSettings?.useWgpuRenderer === true &&
    selectedImage?.isReady &&
    hasRenderedFirstFrame;

  useEffect(() => {
    if (selectedImage && !selectedImage.isReady && selectedImage.path) {
      let isEffectActive = true;
      let fullImageReady = false;
      let prefetchTimer: ReturnType<typeof setTimeout> | null = null;

      const loadFastEmbeddedPreview = async () => {
        try {
          const previewDataUrl = await invoke<string>('extract_embedded_raw_preview', {
            path: selectedImage.path,
          });
          if (!isEffectActive || fullImageReady || !previewDataUrl) return;
          // Only paint embedded camera JPEG if full 32-bit RAW decode hasn't finished yet
          useEditorStore.getState().setEditor((state) => {
            if (state.selectedImage?.path === selectedImage.path && !state.selectedImage.isReady) {
              return { finalPreviewUrl: previewDataUrl };
            }
            return state;
          });
        } catch {
          // Non-fatal: full LoadImage will populate canvas momentarily
        }
      };

      const loadMetadataEarly = async () => {
        try {
          useEditorStore.getState().patchesSentToBackend.clear();
          await invoke('clear_session_caches').catch((e) => console.warn('Cache clear failed:', e));

          const metadata: any = await invoke(Invokes.LoadMetadata, { path: selectedImage.path });
          if (!isEffectActive) return;

          let initialAdjusts;
          if (metadata.adjustments && !metadata.adjustments.is_null) {
            initialAdjusts = normalizeLoadedAdjustments(metadata.adjustments);
          } else {
            initialAdjusts = { ...INITIAL_ADJUSTMENTS };
          }

          setEditor({ adjustments: initialAdjusts });
          resetHistory(initialAdjusts);
        } catch (err) {
          console.error('Failed to load metadata early:', err);
        }
      };

      const loadFullImageData = async () => {
        try {
          const loadImageResult: any = await invoke(Invokes.LoadImage, { path: selectedImage.path });
          if (!isEffectActive) return;
          fullImageReady = true;

          const { width, height } = loadImageResult;
          setEditor({ originalSize: { width, height } });

          if (appSettings?.editorPreviewResolution) {
            const maxSize = appSettings.editorPreviewResolution;
            const aspectRatio = width / height;

            if (width > height) {
              const pWidth = Math.min(width, maxSize);
              const pHeight = Math.round(pWidth / aspectRatio);
              setEditor({ previewSize: { width: pWidth, height: pHeight } });
            } else {
              const pHeight = Math.min(height, maxSize);
              const pWidth = Math.round(pHeight * aspectRatio);
              setEditor({ previewSize: { width: pWidth, height: pHeight } });
            }
          } else {
            setEditor({ previewSize: { width: 0, height: 0 } });
          }

          setEditor((state) => {
            if (state.selectedImage && state.selectedImage.path === selectedImage.path) {
              return {
                selectedImage: {
                  ...state.selectedImage,
                  asShotWhiteBalance: loadImageResult.as_shot_white_balance,
                  exif: loadImageResult.exif,
                  height: loadImageResult.height,
                  isRaw: loadImageResult.is_raw,
                  isReady: true,
                  metadata: loadImageResult.metadata,
                  width: loadImageResult.width,
                },
              };
            }
            return state;
          });

          const existingAdj = loadImageResult.metadata?.adjustments;
          const hasSavedEdits = Boolean(
            existingAdj &&
              !existingAdj.is_null &&
              typeof existingAdj === 'object' &&
              Object.keys(existingAdj).length > 0,
          );

          if (appSettings?.autoAnalyzeOnOpen !== false && !hasSavedEdits) {
            try {
              const polishRes: any = await invoke('analyze_and_polish_active_image', {
                intensity: 100,
                autoStraighten: true,
                toneStyle: 'filmic',
                skinProtection: true,
              });
              if (polishRes?.adjustments && isEffectActive) {
                setEditor((state) => ({
                  adjustments: {
                    ...state.adjustments,
                    ...polishRes.adjustments,
                  },
                }));
                window.dispatchEvent(
                  new CustomEvent('rapidraw-smart-diag', {
                    detail: {
                      sceneName: polishRes.sceneName || 'Universal',
                      adjustments: polishRes.adjustments,
                    },
                  }),
                );
              }
            } catch (_polishErr) {
              try {
                const currentAdj = useEditorStore.getState().adjustments;
                const autoAdjusts: any = await invoke(Invokes.CalculateAutoAdjustments, {
                  jsAdjustments: currentAdj,
                });
                if (autoAdjusts && isEffectActive) {
                  setEditor((state) => ({
                    adjustments: {
                      ...state.adjustments,
                      ...autoAdjusts,
                    },
                  }));
                }
              } catch (autoErr) {
                console.warn('Auto analyze on open skipped or failed:', autoErr);
              }
            }
          }

          if (isEffectActive) {
            prefetchTimer = setTimeout(() => {
              if (!isEffectActive) return;
              const imageList = useLibraryStore.getState().imageList;
              const idx = imageList.findIndex((img) => img.path === selectedImage.path);
              if (idx !== -1 && idx + 1 < imageList.length) {
                const nextImage = imageList[idx + 1];
                if (nextImage?.path) {
                  invoke('prefetch_adjacent_image', { path: nextImage.path }).catch(() => {});
                }
              }
            }, 450);
          }
        } catch (err) {
          if (isEffectActive) {
            console.error('Failed to load image:', err);
            toast.error(`Failed to load image: ${err}`);
            setEditor({ selectedImage: null });
          }
        } finally {
          if (isEffectActive) {
            setLibrary({ isViewLoading: false });
          }
        }
      };

      const loadAll = async () => {
        loadFastEmbeddedPreview();
        await loadMetadataEarly();
        if (isEffectActive) {
          await loadFullImageData();
        }
      };

      loadAll();

      return () => {
        isEffectActive = false;
        if (prefetchTimer) {
          clearTimeout(prefetchTimer);
        }
      };
    }
  }, [
    selectedImage?.path,
    appSettings?.editorPreviewResolution,
    resetHistory,
    setEditor,
    setLibrary,
  ]);

  useEffect(() => {
    if (selectedImage?.path && selectedImage.isReady && (finalPreviewUrl || isWgpuActive)) {
      cachedEditStateRef.current = {
        adjustments,
        histogram,
        waveform,
        finalPreviewUrl,
        uncroppedPreviewUrl: uncroppedAdjustedPreviewUrl,
        selectedImage,
        originalSize,
        previewSize,
      };
    } else {
      cachedEditStateRef.current = null;
    }
  }, [
    selectedImage,
    adjustments,
    histogram,
    waveform,
    finalPreviewUrl,
    uncroppedAdjustedPreviewUrl,
    originalSize,
    previewSize,
    isWgpuActive,
    cachedEditStateRef,
  ]);
}
