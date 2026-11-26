import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { MergeModalState, useUIStore } from '../store/useUIStore';
import { useEditorStore } from '../store/useEditorStore';
import { Invokes } from '../types/invokes';
import { MergeKind, MergePreview, MergeSettings } from '../types/library';
import type { DenoiseMethod } from '../enhanced/modals/DenoiseModal';

export function useProductivityActions(refreshImageList: () => Promise<void> = async () => {}) {
  const setUI = useUIStore((state) => state.setUI);
  const mergeRequest = useRef(0);

  const updateMerge = useCallback(
    (patch: Partial<MergeModalState>) => {
      setUI((state) => ({ mergeModalState: { ...state.mergeModalState, ...patch } }));
    },
    [setUI],
  );

  const handleStartMerge = useCallback(
    async (kindOverride?: MergeKind, pathsOverride?: string[]) => {
      const request = ++mergeRequest.current;
      const { kind, settings, sourcePaths } = useUIStore.getState().mergeModalState;
      const validKind =
        typeof kindOverride === 'string' && ['panorama', 'hdr', 'focusStack'].includes(kindOverride)
          ? (kindOverride as MergeKind)
          : undefined;
      const validPaths = Array.isArray(pathsOverride) ? pathsOverride : undefined;
      const targetKind = validKind ?? kind;
      const targetPaths = validPaths ?? sourcePaths;
      if (targetPaths.length < 2) return;
      updateMerge({
        kind: targetKind,
        sourcePaths: targetPaths,
        error: null,
        isOpen: true,
        isProcessing: true,
        isUpdatingPreview: false,
        preview: null,
        progressMessage: null,
      });
      try {
        const preview: MergePreview = await invoke(Invokes.StartMerge, {
          kind: targetKind,
          paths: targetPaths,
          settings,
        });
        if (request !== mergeRequest.current || !useUIStore.getState().mergeModalState.isOpen) return;
        updateMerge({ isProcessing: false, preview, progressMessage: null });
      } catch (err) {
        if (request !== mergeRequest.current || !useUIStore.getState().mergeModalState.isOpen) return;
        const isCancelled = String(err).toLowerCase().includes('cancel');
        updateMerge({ error: isCancelled ? null : String(err), isProcessing: false, progressMessage: null });
      }
    },
    [updateMerge],
  );

  const handleMergeSettingsChange = useCallback(
    async (settings: MergeSettings) => {
      const { preview, isProcessing, settings: previousSettings } = useUIStore.getState().mergeModalState;
      if (!preview || isProcessing) {
        updateMerge({ settings });
        return;
      }
      const request = ++mergeRequest.current;
      updateMerge({ isUpdatingPreview: true, settings });
      try {
        const nextPreview: MergePreview = await invoke(Invokes.UpdateMergePreview, { settings });
        if (request !== mergeRequest.current) return;
        updateMerge({ isUpdatingPreview: false, preview: nextPreview });
      } catch (err) {
        if (request !== mergeRequest.current) return;
        console.error('Failed to update the merge preview:', err);
        toast.error(String(err));
        updateMerge({ isUpdatingPreview: false, settings: previousSettings });
      }
    },
    [updateMerge],
  );

  const handleSaveMerge = useCallback(async (): Promise<string> => {
    const { settings } = useUIStore.getState().mergeModalState;
    try {
      const savedPath: string = await invoke(Invokes.SaveMerge, { settings });
      await refreshImageList();
      return savedPath;
    } catch (err) {
      console.error('Failed to save the merged image:', err);
      if (useUIStore.getState().mergeModalState.preview) {
        toast.error(`Failed to save merge: ${String(err)}`);
      } else {
        updateMerge({ error: String(err) });
      }
      throw err;
    }
  }, [refreshImageList, updateMerge]);

  const handleSaveCollage = useCallback(
    async (base64Data: string, firstPath: string): Promise<string> => {
      try {
        const savedPath: string = await invoke(Invokes.SaveCollage, { base64Data, firstPathStr: firstPath });
        await refreshImageList();
        return savedPath;
      } catch (err) {
        console.error('Failed to save collage:', err);
        throw err;
      }
    },
    [refreshImageList],
  );

  const handleApplyDenoise = useCallback(
    async (
      intensity: number,
      method: DenoiseMethod,
      healDust = true,
      visualizeDefects = false,
      protectStars = true,
      preserveDetails = 0.25,
      chromaIntensity = 1.0,
      shadowBoost = 0.0,
      deband = false,
      filmGrain = 0.0,
    ) => {
      const { denoiseModalState } = useUIStore.getState();
      if (denoiseModalState.targetPaths.length === 0) return;

      setUI((state) => ({
        denoiseModalState: {
          ...state.denoiseModalState,
          isProcessing: true,
          error: null,
          progressMessage: 'Analyzing image noise profile & denoising...',
        },
      }));

      try {
        const editorState = useEditorStore.getState();
        const targetPath = denoiseModalState.targetPaths[0];
        await invoke(Invokes.ApplyDenoising, {
          path: targetPath,
          intensity,
          method,
          healDust,
          visualizeDefects,
          protectStars,
          preserveDetails,
          chromaIntensity,
          shadowBoost,
          deband,
          filmGrain,
          jsAdjustments: targetPath === editorState.selectedImage?.path ? editorState.adjustments : null,
        });
      } catch (err) {
        const isCancelled = String(err).toLowerCase().includes('cancel');
        setUI((state) => ({
          denoiseModalState: {
            ...state.denoiseModalState,
            isProcessing: false,
            progressMessage: null,
            error: isCancelled ? null : String(err),
          },
        }));
      }
    },
    [setUI],
  );

  const handleBatchDenoise = useCallback(
    async (
      intensity: number,
      method: DenoiseMethod,
      paths: string[],
      healDust = true,
      protectStars = true,
      preserveDetails = 0.25,
      chromaIntensity = 1.0,
      shadowBoost = 0.0,
      deband = false,
      filmGrain = 0.0,
    ) => {
      try {
        const savedPaths: string[] = await invoke(Invokes.BatchDenoiseImages, {
          paths,
          intensity,
          method,
          healDust,
          protectStars,
          preserveDetails,
          chromaIntensity,
          shadowBoost,
          deband,
          filmGrain,
        });
        await refreshImageList();
        return savedPaths;
      } catch (err) {
        const isCancelled = String(err).toLowerCase().includes('cancel');
        setUI((state) => ({
          denoiseModalState: {
            ...state.denoiseModalState,
            isProcessing: false,
            progressMessage: null,
            error: isCancelled ? null : String(err),
          },
        }));
        throw err;
      }
    },
    [refreshImageList, setUI],
  );

  const handleSaveDenoisedImage = useCallback(async (): Promise<string> => {
    const { denoiseModalState } = useUIStore.getState();
    if (denoiseModalState.targetPaths.length === 0) throw new Error('No target path');
    const savedPath = await invoke<string>(Invokes.SaveDenoisedImage, {
      originalPathStr: denoiseModalState.targetPaths[0],
    });
    await refreshImageList();
    return savedPath;
  }, [refreshImageList]);

  const handleStartStockPhotoPrep = useCallback(
    async (
      paths: string[],
      outputDir: string,
      customOptions?: {
        enableBm3dTriad?: boolean;
        enableReflector?: boolean;
        enableAutoFraming?: boolean;
        enableDustScrubbing?: boolean;
        enableMultiCrop?: boolean;
        enableBlinkGate?: boolean;
        enableAgencyDispatch?: boolean;
      },
    ) => {
      const result: any = await invoke(Invokes.BatchStockPhotoPrep, {
        options: {
          input_paths: paths,
          output_dir: outputDir,
          format: 'jpg',
          quality: 95,
          enable_bm3d_triad: customOptions?.enableBm3dTriad ?? true,
          enable_reflector: customOptions?.enableReflector ?? true,
          enable_auto_framing: customOptions?.enableAutoFraming ?? true,
          enable_dust_scrubbing: customOptions?.enableDustScrubbing ?? true,
          enable_multi_crop: customOptions?.enableMultiCrop ?? false,
          enable_blink_gate: customOptions?.enableBlinkGate ?? true,
          enable_agency_dispatch: customOptions?.enableAgencyDispatch ?? false,
        },
      });
      await refreshImageList();
      return result;
    },
    [refreshImageList],
  );

  const handleProcessNightSkySession = useCallback(
    async (
      paths: string[],
      options?: {
        freezeGround?: boolean;
        removeLightPollution?: boolean;
        sigmaClip?: number;
        starTrailsMode?: boolean;
        cometDecay?: boolean;
        decayRate?: number;
        fillGaps?: boolean;
        useGpu?: boolean;
      },
    ) => {
      if (paths.length < 2) return;
      setUI((state) => ({
        nightSkyState: {
          ...state.nightSkyState,
          isProcessing: true,
          error: null,
          progressMessage: options?.starTrailsMode ? 'Initializing Star Trails Engine...' : 'Initializing Night Sky Stacker...',
        },
      }));
      try {
        if (options?.starTrailsMode) {
          await invoke(Invokes.StackStarTrails, {
            paths,
            cometDecay: options?.cometDecay ?? true,
            decayRate: options?.decayRate ?? 0.08,
            fillGaps: options?.fillGaps ?? true,
          });
        } else {
          await invoke(Invokes.StackAstroFrames, {
            options: {
              paths,
              sigma_clip: options?.sigmaClip ?? 2.5,
              stack_mode: 'kappa_sigma',
              auto_dark_subtract: true,
              remove_light_pollution: options?.removeLightPollution ?? true,
              freeze_ground: options?.freezeGround ?? true,
              use_gpu: options?.useGpu ?? true,
            },
          });
        }
        setUI((state) => ({
          nightSkyState: { ...state.nightSkyState, isProcessing: false, progressMessage: 'Session Stack Complete!' },
        }));
      } catch (err) {
        const isCancelled = String(err).toLowerCase().includes('cancel');
        setUI((state) => ({
          nightSkyState: {
            ...state.nightSkyState,
            isProcessing: false,
            progressMessage: null,
            error: isCancelled ? null : String(err),
          },
        }));
      }
    },
    [setUI],
  );

  return {
    handleMergeSettingsChange,
    handleSaveCollage,
    handleSaveMerge,
    handleStartMerge,
    handleApplyDenoise,
    handleBatchDenoise,
    handleSaveDenoisedImage,
    handleStartStockPhotoPrep,
    handleProcessNightSkySession,
  };
}
