import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import i18n from 'i18next';
import { toast } from 'react-toastify';
import { Invokes } from '../types/invokes';
import {
  ImageFile,
  CullingSuggestions,
  DEFAULT_MERGE_SETTINGS,
  MergeKind,
  MergePreview,
  MergeSettings,
} from '../types/library';
import { Panel, UiVisibility, PanelRegion, CollapsibleSectionsState } from '../types/ui';
import { SavedWorkspacePreset, WorkspaceState } from '../types/settings';
import { useEditorStore } from './useEditorStore';
import { useSettingsStore } from './useSettingsStore';
import { pickCropGeometry } from '../utils/adjustments';

export type SwitcherPlacement = 'bottom' | 'right' | 'left' | 'top';

export type QuickEnhanceAction =
  | 'smart-tone'
  | 'color-contrast'
  | 'diffusion'
  | 'supercontrast'
  | 'retouch'
  | 'liquify';

export interface CropSectionsState {
  transform: boolean;
  lens: boolean;
}

interface ConfirmModalState {
  confirmText?: string;
  confirmVariant?: string;
  isOpen: boolean;
  message?: string;
  onConfirm?(): void;
  title?: string;
}

interface CollageModalState {
  isOpen: boolean;
  sourceImages: Array<Pick<ImageFile, 'path'>>;
}

export interface MergeModalState {
  error: string | null;
  isOpen: boolean;
  isProcessing: boolean;
  isUpdatingPreview: boolean;
  kind: MergeKind;
  preview: MergePreview | null;
  progressMessage: string | null;
  settings: MergeSettings;
  sourcePaths: Array<string>;
}

export const CLOSED_MERGE_MODAL_STATE: MergeModalState = {
  error: null,
  isOpen: false,
  isProcessing: false,
  isUpdatingPreview: false,
  kind: 'panorama',
  preview: null,
  progressMessage: null,
  settings: DEFAULT_MERGE_SETTINGS.panorama,
  sourcePaths: [],
};

export interface DrizzleModalState {
  isOpen: boolean;
  isProcessing: boolean;
  sourcePaths: string[];
  scaleFactor: number;
  pixfrac: number;
  bayerCfaMode?: boolean;
  alignmentMode?: 'auto' | 'terrestrial' | 'stars';
  progressMessage: string | null;
  error: string | null;
  finalImageBase64: string | null;
  originalBase64: string | null;
  meta: {
    width: number;
    height: number;
    scale: number;
    frames_stacked: number;
    snr_boost: string;
  } | null;
}

export interface DenoiseModalState {
  isOpen: boolean;
  isProcessing: boolean;
  previewBase64: string | null;
  originalBase64?: string | null;
  error: string | null;
  targetPaths: string[];
  progressMessage: string | null;
  isRaw: boolean;
}

export interface NegativeConversionModalState {
  isOpen: boolean;
  targetPaths: Array<string>;
}

export interface BatchPolishModalState {
  isOpen: boolean;
  selectedPaths: Array<string>;
}

export interface ColorMatcherModalState {
  isOpen: boolean;
  heroPath?: string | null;
  targetPaths?: string[];
  initialMode?: 'matcher' | 'harmonize';
}

export interface HeroCuratorModalState {
  isOpen: boolean;
  selectedPaths: Array<string>;
}

export interface ClientDeliveryModalState {
  isOpen: boolean;
  selectedPaths: Array<string>;
}

export interface BokehModalState {
  isOpen: boolean;
}

export interface AdvancedExportModalState {
  isOpen: boolean;
  selectedPaths: Array<string>;
}

export interface SpeedCullerModalState {
  isOpen: boolean;
  selectedPaths: Array<string>;
  initialIndex: number;
}

export interface NightSkyState {
  isExpanded: boolean;
  targetPaths: Array<string>;
  isProcessing: boolean;
  progressMessage: string | null;
  freezeGround: boolean;
  removeLightPollution: boolean;
  sigmaClip: number;
  error: string | null;
  starTrailsMode: boolean;
  cometDecay: boolean;
  decayRate: number;
  fillGaps: boolean;
  useGpu: boolean;
}

interface CullingModalState {
  isOpen: boolean;
  suggestions: CullingSuggestions | null;
  progress: { current: number; total: number; stage: string } | null;
  error: string | null;
  pathsToCull: Array<string>;
}

const ALL_PANELS: Panel[] = [
  Panel.Metadata,
  Panel.FolderTree,
  Panel.Tones,
  Panel.Export,
  Panel.Tethering,
  Panel.Adjustments,
  Panel.Crop,
  Panel.Masks,
  Panel.Ai,
  Panel.Presets,
];

const PANEL_REGIONS: PanelRegion[] = ['leftTop', 'leftBottom', 'rightTop', 'rightBottom'];

const SIBLING_REGION: Record<PanelRegion, PanelRegion> = {
  leftTop: 'leftBottom',
  leftBottom: 'leftTop',
  rightTop: 'rightBottom',
  rightBottom: 'rightTop',
};

const DEFAULT_PANEL_DEFAULT_REGIONS: Partial<Record<Panel, PanelRegion>> = {
  [Panel.Metadata]: 'leftTop',
  [Panel.FolderTree]: 'leftTop',
  [Panel.Tones]: 'leftTop',
  [Panel.Export]: 'leftTop',
  [Panel.Tethering]: 'leftTop',
  [Panel.Adjustments]: 'rightTop',
  [Panel.Crop]: 'rightTop',
  [Panel.Masks]: 'rightTop',
  [Panel.Ai]: 'rightTop',
  [Panel.Presets]: 'rightTop',
};

// Lenovo ThinkPad L13 Gen 2 (13.3" <= 1599px CSS width): compact 285px sidebars leave >790px canvas.
export const DEFAULT_PANEL_WIDTH =
  typeof window !== 'undefined' && window.innerWidth < 1600 ? 285 : 350;
export const DEFAULT_PANEL_SECTION_HEIGHT = 450;
export const DEFAULT_BOTTOM_PANEL_HEIGHT = 144;
export const DEFAULT_FILMSTRIP_COLUMNS = 2;
export const MAX_FILMSTRIP_COLUMNS = 6;
const MIN_REOPENED_PANEL_WIDTH = 250;

export const getReopenedPanelWidth = (width: number) =>
  width < MIN_REOPENED_PANEL_WIDTH ? DEFAULT_PANEL_WIDTH : width;

export function reconcileWorkspace(
  savedWorkspace: WorkspaceState | undefined,
  isTetheringSupported: boolean,
): WorkspaceState {
  const defaultPlacedPanels = ALL_PANELS.filter((p) => p !== Panel.Tethering || isTetheringSupported);
  const allowedPanels = new Set([...defaultPlacedPanels, Panel.Filmstrip]);

  const defaultWorkspace: WorkspaceState = {
    leftPanelWidth: DEFAULT_PANEL_WIDTH,
    rightPanelWidth: DEFAULT_PANEL_WIDTH,
    leftTopHeight: DEFAULT_PANEL_SECTION_HEIGHT,
    rightTopHeight: DEFAULT_PANEL_SECTION_HEIGHT,
    panelLayout: {
      leftTop: [Panel.Metadata, Panel.FolderTree, Panel.Tones, Panel.Export, ...(isTetheringSupported ? [Panel.Tethering] : [])],
      leftBottom: [],
      rightTop: [Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai, Panel.Presets],
      rightBottom: [],
    },
    activePanels: {
      leftTop: Panel.FolderTree,
      leftBottom: null,
      rightTop: Panel.Adjustments,
      rightBottom: null,
    },
    panelSwitcherPlacement: {
      leftTop: 'bottom',
      leftBottom: 'bottom',
      rightTop: 'right',
      rightBottom: 'right',
    },
    filmstripColumns: DEFAULT_FILMSTRIP_COLUMNS,
    filmstripShowFileInfo: false,
  };

  if (!savedWorkspace || !savedWorkspace.panelLayout) {
    return defaultWorkspace;
  }

  const seenPanels = new Set<Panel>();
  const sanitizedLayout: Record<PanelRegion, Panel[]> = {
    leftTop: [],
    leftBottom: [],
    rightTop: [],
    rightBottom: [],
  };

  PANEL_REGIONS.forEach((region) => {
    const list = savedWorkspace.panelLayout[region] || [];
    list.forEach((panel) => {
      if (allowedPanels.has(panel) && !seenPanels.has(panel)) {
        sanitizedLayout[region].push(panel);
        seenPanels.add(panel);
      }
    });
  });

  defaultPlacedPanels.forEach((panel) => {
    if (!seenPanels.has(panel)) {
      const targetRegion = DEFAULT_PANEL_DEFAULT_REGIONS[panel] || 'leftTop';
      sanitizedLayout[targetRegion].push(panel);
      seenPanels.add(panel);
    }
  });

  const sanitizedActive: Record<PanelRegion, Panel | null> = {
    leftTop: null,
    leftBottom: null,
    rightTop: null,
    rightBottom: null,
  };

  PANEL_REGIONS.forEach((region) => {
    const currentActive = savedWorkspace.activePanels?.[region];
    if (currentActive && sanitizedLayout[region].includes(currentActive)) {
      sanitizedActive[region] = currentActive;
    } else {
      sanitizedActive[region] = sanitizedLayout[region].length > 0 ? sanitizedLayout[region][0] : null;
    }
  });

  return {
    leftPanelWidth: savedWorkspace.leftPanelWidth || defaultWorkspace.leftPanelWidth,
    rightPanelWidth: savedWorkspace.rightPanelWidth || defaultWorkspace.rightPanelWidth,
    leftTopHeight: savedWorkspace.leftTopHeight || defaultWorkspace.leftTopHeight,
    rightTopHeight: savedWorkspace.rightTopHeight || defaultWorkspace.rightTopHeight,
    panelLayout: sanitizedLayout,
    activePanels: sanitizedActive,
    panelSwitcherPlacement: {
      ...defaultWorkspace.panelSwitcherPlacement,
      ...(savedWorkspace.panelSwitcherPlacement || {}),
    },
    filmstripColumns: Math.min(
      MAX_FILMSTRIP_COLUMNS,
      Math.max(1, savedWorkspace.filmstripColumns || DEFAULT_FILMSTRIP_COLUMNS),
    ),
    filmstripShowFileInfo: savedWorkspace.filmstripShowFileInfo ?? false,
  };
}

export const isPanelVisible = (state: UIState, panel: Panel) =>
  (Object.keys(state.activePanels) as PanelRegion[]).some(
    (region) =>
      state.activePanels[region] === panel &&
      state.uiVisibility[region.startsWith('left') ? 'leftPanel' : 'rightPanel'],
  );

export const isFilmstripDocked = (state: UIState) =>
  PANEL_REGIONS.some((region) => state.panelLayout[region].includes(Panel.Filmstrip));

export const isPanelAvailable = (state: UIState, panel: Panel) =>
  panel !== Panel.Filmstrip || state.activeView === 'editor';

export const getRegionActivePanel = (state: UIState, region: PanelRegion) => {
  const panels = state.panelLayout[region];
  const active = state.activePanels[region];
  const index = active ? panels.indexOf(active) : -1;
  if (active && index >= 0 && isPanelAvailable(state, active)) return active;
  const candidates = index < 0 ? panels : [...panels.slice(index + 1), ...panels.slice(0, index).reverse()];
  return candidates.find((panel) => isPanelAvailable(state, panel)) ?? null;
};

const removePanelFromLayout = (state: UIState, panel: Panel) => {
  const layout = Object.fromEntries(
    PANEL_REGIONS.map((region) => [region, state.panelLayout[region].filter((p) => p !== panel)]),
  ) as Record<PanelRegion, Panel[]>;
  const active = { ...state.activePanels };
  const fromRegion = PANEL_REGIONS.find((region) => state.panelLayout[region].includes(panel));
  let revealedPanel: Panel | null = null;
  if (fromRegion && active[fromRegion] === panel) {
    const original = state.panelLayout[fromRegion];
    const index = original.indexOf(panel);
    revealedPanel = original[index + 1] ?? original[index - 1] ?? null;
    active[fromRegion] = revealedPanel;
  }
  return { layout, active, fromRegion, revealedPanel };
};

const resolveGlobalPanel = (current: Panel | null, displaced: Panel | null, panel: Panel) => {
  if (panel !== Panel.Filmstrip) return panel;
  return displaced === current ? null : current;
};

interface PlacePanelOptions {
  index?: number;
  split?: boolean;
  placement?: SwitcherPlacement | null;
}

const placePanel = (
  state: UIState,
  panel: Panel,
  region: PanelRegion,
  { index = Number.MAX_SAFE_INTEGER, split = false, placement = null }: PlacePanelOptions,
) => {
  const { layout, active, fromRegion, revealedPanel } = removePanelFromLayout(state, panel);
  const placements = { ...state.panelSwitcherPlacement };

  const sibling = SIBLING_REGION[region];
  if (split && layout[region].length > 0 && layout[sibling].length === 0) {
    [layout[region], layout[sibling]] = [layout[sibling], layout[region]];
    [active[region], active[sibling]] = [active[sibling], active[region]];
    [placements[region], placements[sibling]] = [placements[sibling], placements[region]];
  }

  const displacedPanel = active[region];
  layout[region].splice(Math.max(0, Math.min(index, layout[region].length)), 0, panel);
  active[region] = panel;
  if (placement) placements[region] = placement;

  const globalPanel =
    resolveGlobalPanel(state.activePanel, displacedPanel, panel) ?? (fromRegion !== region ? revealedPanel : null);
  return {
    panelLayout: layout,
    activePanels: active,
    panelSwitcherPlacement: placements,
    activeLayoutDragItem: null,
    activePanel: globalPanel,
    renderedPanel: globalPanel,
  };
};

export interface UIState {
  activeView: string;
  isFullScreen: boolean;
  isWindowFullScreen: boolean;
  isViewerOpen: boolean;
  isInstantTransition: boolean;
  isLayoutReady: boolean;
  uiVisibility: UiVisibility;
  isLibraryExportPanelVisible: boolean;
  isSettingsOpen: boolean;

  leftPanelWidth: number;
  rightPanelWidth: number;
  bottomPanelHeight: number;
  leftTopHeight: number;
  rightTopHeight: number;
  filmstripColumns: number;
  filmstripShowFileInfo: boolean;
  compactEditorPanelHeightOverride: number | null;

  panelLayout: Record<PanelRegion, Panel[]>;
  activePanels: Record<PanelRegion, Panel | null>;
  activeLayoutDragItem: Panel | null;
  setLayoutDragItem: (panel: Panel | null) => void;
  dropPanel: (
    panel: Panel,
    target: { region: PanelRegion; split: boolean; placement: SwitcherPlacement | null },
  ) => void;
  movePanelToIndex: (panel: Panel, toRegion: PanelRegion, index: number) => void;
  dockFilmstripToBottom: () => void;
  setActivePanel: (region: PanelRegion, panel: Panel | null) => void;

  panelSwitcherPlacement: Record<PanelRegion, SwitcherPlacement>;

  activePanel: Panel | null;
  renderedPanel: Panel | null;
  slideDirection: number;
  collapsibleSectionsState: CollapsibleSectionsState;
  showActiveToolsOnly: boolean;
  pendingQuickAction: QuickEnhanceAction | null;
  triggerQuickAction: (panel: Panel, action: QuickEnhanceAction) => void;
  consumeQuickAction: () => QuickEnhanceAction | null;
  isColorGradingExpanded: boolean;
  isColorMixerExpanded: boolean;
  cropSectionsState: CropSectionsState;

  isCreateFolderModalOpen: boolean;
  isRenameFolderModalOpen: boolean;
  isRenameFileModalOpen: boolean;
  renameTargetPaths: Array<string>;
  isImportModalOpen: boolean;
  isCopyPasteSettingsModalOpen: boolean;
  importTargetFolder: string | null;
  importSourcePaths: Array<string>;
  folderActionTarget: string | null;

  isCreateAlbumModalOpen: boolean;
  isCreateAlbumGroupModalOpen: boolean;
  isRenameAlbumModalOpen: boolean;
  albumActionTarget: string | null;

  confirmModalState: ConfirmModalState;
  mergeModalState: MergeModalState;
  drizzleModalState: DrizzleModalState;
  negativeModalState: NegativeConversionModalState;
  denoiseModalState: DenoiseModalState;
  cullingModalState: CullingModalState;
  batchPolishModalState: BatchPolishModalState;
  colorMatcherModalState: ColorMatcherModalState;
  heroCuratorModalState: HeroCuratorModalState;
  clientDeliveryModalState: ClientDeliveryModalState;
  bokehModalState: BokehModalState;
  advancedExportModalState: AdvancedExportModalState;
  speedCullerModalState: SpeedCullerModalState;
  collageModalState: CollageModalState;
  nightSkyState: NightSkyState;
  softProofState: {
    enabled: boolean;
    target: string;
    intent: string;
    oogWarning: boolean;
    customIccPath: string | null;
  };
  isLensCorrectionModalOpen: boolean;
  isTransformModalOpen: boolean;

  setUI: (updater: Partial<UIState> | ((state: UIState) => Partial<UIState>)) => void;
  setPanel: (panel: Panel | null) => void;
  customEscapeHandler: (() => void) | null;
  setCustomEscapeHandler: (handler: (() => void) | null) => void;
  imageSelectHandler: ((path: string, openInEditor?: boolean) => void) | null;
  setImageSelectHandler: (handler: ((path: string, openInEditor?: boolean) => void) | null) => void;
  maskOverlayMode: 'red' | 'green' | 'bw' | 'unmasked';
  searchFocusRequest: number;
  requestSearchFocus: () => void;
  toggleFullScreen: () => void;
  toggleViewerWindow: () => void;
  resetWorkspaceLayout: (isTetheringSupported?: boolean) => WorkspaceState;
  applyWorkspacePreset: (presetId: string) => void;
  saveCurrentWorkspacePreset: (name: string) => void;
}

export const useUIStore = create<UIState>((set, get) => ({
  activeView: 'library',
  isFullScreen: false,
  isWindowFullScreen: false,
  isViewerOpen: false,
  isInstantTransition: false,
  isLayoutReady: false,
  uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: false },
  isLibraryExportPanelVisible: false,
  isSettingsOpen: false,

  leftPanelWidth: DEFAULT_PANEL_WIDTH,
  rightPanelWidth: DEFAULT_PANEL_WIDTH,
  bottomPanelHeight: DEFAULT_BOTTOM_PANEL_HEIGHT,
  leftTopHeight: DEFAULT_PANEL_SECTION_HEIGHT,
  rightTopHeight: DEFAULT_PANEL_SECTION_HEIGHT,
  filmstripColumns: DEFAULT_FILMSTRIP_COLUMNS,
  filmstripShowFileInfo: false,
  compactEditorPanelHeightOverride: null,

  panelLayout: {
    leftTop: [Panel.Metadata, Panel.FolderTree, Panel.Tones, Panel.Export],
    leftBottom: [],
    rightTop: [Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai, Panel.Presets],
    rightBottom: [],
  },
  activePanels: {
    leftTop: Panel.FolderTree,
    leftBottom: null,
    rightTop: Panel.Adjustments,
    rightBottom: null,
  },
  activeLayoutDragItem: null,

  panelSwitcherPlacement: {
    leftTop: 'bottom',
    leftBottom: 'bottom',
    rightTop: 'right',
    rightBottom: 'right',
  },

  activePanel: Panel.Adjustments,
  renderedPanel: Panel.Adjustments,
  slideDirection: 1,
  collapsibleSectionsState: { basic: true, color: false, curves: true, details: false, effects: false },
  showActiveToolsOnly: false,
  pendingQuickAction: null,
  isColorGradingExpanded: false,
  isColorMixerExpanded: false,
  cropSectionsState: { transform: false, lens: false },

  isCreateFolderModalOpen: false,
  isRenameFolderModalOpen: false,
  isRenameFileModalOpen: false,
  renameTargetPaths: [],
  isImportModalOpen: false,
  isCopyPasteSettingsModalOpen: false,
  importTargetFolder: null,
  importSourcePaths: [],
  folderActionTarget: null,
  isCreateAlbumModalOpen: false,
  isCreateAlbumGroupModalOpen: false,
  isRenameAlbumModalOpen: false,
  albumActionTarget: null,

  confirmModalState: { isOpen: false },
  mergeModalState: CLOSED_MERGE_MODAL_STATE,
  drizzleModalState: {
    isOpen: false,
    isProcessing: false,
    sourcePaths: [],
    scaleFactor: 2,
    pixfrac: 0.8,
    bayerCfaMode: true,
    alignmentMode: 'auto',
    progressMessage: null,
    error: null,
    finalImageBase64: null,
    originalBase64: null,
    meta: null,
  },
  negativeModalState: { isOpen: false, targetPaths: [] },
  denoiseModalState: {
    isOpen: false,
    isProcessing: false,
    previewBase64: null,
    error: null,
    targetPaths: [],
    progressMessage: null,
    isRaw: false,
  },
  batchPolishModalState: { isOpen: false, selectedPaths: [] },
  colorMatcherModalState: { isOpen: false },
  heroCuratorModalState: { isOpen: false, selectedPaths: [] },
  clientDeliveryModalState: { isOpen: false, selectedPaths: [] },
  bokehModalState: { isOpen: false },
  advancedExportModalState: { isOpen: false, selectedPaths: [] },
  speedCullerModalState: { isOpen: false, selectedPaths: [], initialIndex: 0 },
  nightSkyState: {
    isExpanded: false,
    targetPaths: [],
    isProcessing: false,
    progressMessage: null,
    freezeGround: true,
    removeLightPollution: true,
    sigmaClip: 2.5,
    error: null,
    starTrailsMode: false,
    cometDecay: true,
    decayRate: 0.08,
    fillGaps: true,
    useGpu: true,
  },
  softProofState: {
    enabled: false,
    target: 'srgb',
    intent: 'relativeColorimetric',
    oogWarning: true,
    customIccPath: null,
  },
  isLensCorrectionModalOpen: false,
  isTransformModalOpen: false,
  cullingModalState: { isOpen: false, suggestions: null, progress: null, error: null, pathsToCull: [] },
  collageModalState: { isOpen: false, sourceImages: [] },

  setUI: (updater) => set((state) => (typeof updater === 'function' ? updater(state) : updater)),

  setLayoutDragItem: (panel) => set({ activeLayoutDragItem: panel }),

  dropPanel: (panel, { region, split, placement }) =>
    set((state) => {
      const currentIndex = state.panelLayout[region].indexOf(panel);
      return placePanel(state, panel, region, {
        index: !split && currentIndex >= 0 ? currentIndex : undefined,
        split,
        placement,
      });
    }),

  movePanelToIndex: (panel, toRegion, index) => set((state) => placePanel(state, panel, toRegion, { index })),

  dockFilmstripToBottom: () =>
    set((state) => {
      const { layout, active, revealedPanel } = removePanelFromLayout(state, Panel.Filmstrip);
      const globalPanel = state.activePanel ?? revealedPanel;
      return {
        panelLayout: layout,
        activePanels: active,
        activeLayoutDragItem: null,
        activePanel: globalPanel,
        renderedPanel: globalPanel,
        uiVisibility: { ...state.uiVisibility, filmstrip: true },
      };
    }),

  setActivePanel: (region, panel) =>
    set((state) => {
      if (!panel) return state;
      const globalPanel = resolveGlobalPanel(state.activePanel, state.activePanels[region], panel);
      const updates: Partial<UIState> = {
        activePanels: { ...state.activePanels, [region]: panel },
        activePanel: globalPanel,
        renderedPanel: globalPanel,
      };

      const isLeft = region === 'leftTop' || region === 'leftBottom';
      const isRight = region === 'rightTop' || region === 'rightBottom';

      if (isLeft && !state.uiVisibility.leftPanel) {
        updates.uiVisibility = { ...state.uiVisibility, leftPanel: true };
        if (state.leftPanelWidth < DEFAULT_PANEL_WIDTH) {
          updates.leftPanelWidth = DEFAULT_PANEL_WIDTH;
        }
      }

      if (isRight && !state.uiVisibility.rightPanel) {
        updates.uiVisibility = { ...state.uiVisibility, rightPanel: true };
        if (state.rightPanelWidth < DEFAULT_PANEL_WIDTH) {
          updates.rightPanelWidth = DEFAULT_PANEL_WIDTH;
        }
      }

      return updates;
    }),

  setPanel: (panelId) => {
    const state = get();
    if (!panelId) return;

    let targetRegion: PanelRegion | null = null;
    for (const region of Object.keys(state.panelLayout) as PanelRegion[]) {
      if (state.panelLayout[region].includes(panelId)) {
        targetRegion = region;
        break;
      }
    }
    if (targetRegion) state.setActivePanel(targetRegion, panelId);
  },

  triggerQuickAction: (panel, action) => {
    const state = get();
    const isPlaced = PANEL_REGIONS.some((region) => state.panelLayout[region].includes(panel));
    if (!isPlaced) {
      state.movePanelToIndex(panel, 'rightTop', state.panelLayout.rightTop.length);
    }
    get().setPanel(panel);
    set((s) => ({
      pendingQuickAction: action,
      ...(panel === Panel.Adjustments
        ? { collapsibleSectionsState: { ...s.collapsibleSectionsState, basic: true } }
        : {}),
    }));
  },

  consumeQuickAction: () => {
    const action = get().pendingQuickAction;
    if (action !== null) {
      set({ pendingQuickAction: null });
    }
    return action;
  },

  toggleFullScreen: () => {
    const { isFullScreen } = get();
    const { zoom, selectedImage } = useEditorStore.getState();

    const isNotFitToScreen = Math.abs(zoom - 1) > 0.01;
    set({ isInstantTransition: isNotFitToScreen });

    if (isFullScreen) {
      set({ isFullScreen: false });
    } else {
      if (!selectedImage) return;
      set({ isFullScreen: true });
    }

    if (isNotFitToScreen) {
      setTimeout(() => set({ isInstantTransition: false }), 100);
    }
  },

  toggleViewerWindow: () => {
    if (get().isViewerOpen) {
      set({ isViewerOpen: false });
      invoke(Invokes.CloseViewerWindow).catch(console.error);
      return;
    }
    invoke(Invokes.OpenViewerWindow, { title: i18n.t('editor.viewer.windowTitle') })
      .then(() => set({ isViewerOpen: true }))
      .catch((err) => toast.error(i18n.t('editor.viewer.openFailed', { error: String(err) })));
  },

  resetWorkspaceLayout: (isTetheringSupported = false) => {
    const defaultWorkspace = reconcileWorkspace(undefined, isTetheringSupported);
    set({
      leftPanelWidth: defaultWorkspace.leftPanelWidth,
      rightPanelWidth: defaultWorkspace.rightPanelWidth,
      leftTopHeight: defaultWorkspace.leftTopHeight,
      rightTopHeight: defaultWorkspace.rightTopHeight,
      panelLayout: defaultWorkspace.panelLayout,
      activePanels: defaultWorkspace.activePanels,
      panelSwitcherPlacement: defaultWorkspace.panelSwitcherPlacement,
      filmstripColumns: defaultWorkspace.filmstripColumns,
      filmstripShowFileInfo: defaultWorkspace.filmstripShowFileInfo,
      uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: false },
      activePanel: defaultWorkspace.activePanels.rightTop || null,
      renderedPanel: defaultWorkspace.activePanels.rightTop || null,
    });
    return defaultWorkspace;
  },

  applyWorkspacePreset: (presetId: string) => {
    const state = get();
    const settingsState = useSettingsStore.getState();
    const appSettings = settingsState.appSettings;
    const isTetheringSupported = Boolean(appSettings?.workspace?.panelLayout?.leftTop?.includes(Panel.Tethering));
    const base = reconcileWorkspace(undefined, isTetheringSupported);

    const custom = appSettings?.savedWorkspacePresets?.find((p) => p.id === presetId);
    const builtin = BUILTIN_WORKSPACE_PRESETS.find((p) => p.id === presetId);

    const chosen: SavedWorkspacePreset | undefined =
      custom ??
      (builtin
        ? {
            id: builtin.id,
            name: builtin.name,
            workspace: {
              ...base,
              ...builtin.workspacePatch,
              activePanels: { ...base.activePanels, ...(builtin.workspacePatch.activePanels ?? {}) },
            },
            hideCollapsedPanels: builtin.hideCollapsedPanels,
            activeToolsOnly: builtin.activeToolsOnly,
            bottomPanelHeight: builtin.bottomPanelHeight,
          }
        : undefined);

    if (!chosen) return;

    const ws = chosen.workspace;
    set({
      leftPanelWidth: ws.leftPanelWidth,
      rightPanelWidth: ws.rightPanelWidth,
      leftTopHeight: ws.leftTopHeight,
      rightTopHeight: ws.rightTopHeight,
      panelLayout: ws.panelLayout,
      activePanels: ws.activePanels,
      panelSwitcherPlacement: ws.panelSwitcherPlacement,
      filmstripColumns: ws.filmstripColumns,
      filmstripShowFileInfo: ws.filmstripShowFileInfo,
      bottomPanelHeight: chosen.bottomPanelHeight ?? state.bottomPanelHeight,
      showActiveToolsOnly: chosen.activeToolsOnly,
      activePanel: ws.activePanels.rightTop || ws.activePanels.rightBottom || state.activePanel,
      renderedPanel: ws.activePanels.rightTop || ws.activePanels.rightBottom || state.renderedPanel,
      uiVisibility: builtin?.uiVisibility ?? { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: false },
    });

    if (appSettings) {
      settingsState.handleSettingsChange({
        ...appSettings,
        workspace: ws,
        hideCollapsedPanels: chosen.hideCollapsedPanels,
        enableToolFocusMode: chosen.activeToolsOnly,
        activeWorkspacePreset: chosen.id,
      });
    }
  },

  saveCurrentWorkspacePreset: (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const state = get();
    const settingsState = useSettingsStore.getState();
    const appSettings = settingsState.appSettings;
    if (!appSettings) return;

    const currentWs: WorkspaceState = {
      leftPanelWidth: state.leftPanelWidth,
      rightPanelWidth: state.rightPanelWidth,
      leftTopHeight: state.leftTopHeight,
      rightTopHeight: state.rightTopHeight,
      panelLayout: state.panelLayout,
      activePanels: state.activePanels,
      panelSwitcherPlacement: state.panelSwitcherPlacement,
      filmstripColumns: state.filmstripColumns,
      filmstripShowFileInfo: state.filmstripShowFileInfo,
    };

    const newPreset: SavedWorkspacePreset = {
      id: `custom_${crypto.randomUUID().slice(0, 8)}`,
      name: trimmed,
      workspace: currentWs,
      hideCollapsedPanels: Boolean(appSettings.hideCollapsedPanels),
      activeToolsOnly: state.showActiveToolsOnly,
      bottomPanelHeight: state.bottomPanelHeight,
    };

    const existing = (appSettings.savedWorkspacePresets ?? []).filter(
      (p) => p.name.toLowerCase() !== trimmed.toLowerCase(),
    );

    settingsState.handleSettingsChange({
      ...appSettings,
      savedWorkspacePresets: [...existing, newPreset],
      activeWorkspacePreset: newPreset.id,
    });
  },

  customEscapeHandler: null,
  setCustomEscapeHandler: (handler) => set({ customEscapeHandler: handler }),
  imageSelectHandler: null,
  setImageSelectHandler: (handler) => set({ imageSelectHandler: handler }),
  maskOverlayMode: 'red',
  searchFocusRequest: 0,
  requestSearchFocus: () => set((state) => ({ searchFocusRequest: state.searchFocusRequest + 1 })),
}));

useUIStore.subscribe((state, prev) => {
  if (state.activePanel === prev.activePanel) return;
  const { selectedImage, adjustments, setEditor } = useEditorStore.getState();
  if (state.activePanel === Panel.Crop) {
    setEditor({
      cropSessionSnapshot: { path: selectedImage?.path ?? null, geometry: pickCropGeometry(adjustments) },
    });
  } else if (prev.activePanel === Panel.Crop) {
    setEditor({ cropSessionSnapshot: null });
  }
});

useEditorStore.subscribe((state, prev) => {
  if (state.selectedImage?.path !== prev.selectedImage?.path && useUIStore.getState().pendingQuickAction !== null) {
    useUIStore.setState({ pendingQuickAction: null });
  }
});

export const BUILTIN_WORKSPACE_PRESETS: Array<{
  id: string;
  name: string;
  workspacePatch: Partial<WorkspaceState>;
  hideCollapsedPanels: boolean;
  activeToolsOnly: boolean;
  bottomPanelHeight?: number;
  uiVisibility?: UiVisibility;
}> = [
  {
    id: 'default',
    name: 'Default',
    workspacePatch: {},
    hideCollapsedPanels: false,
    activeToolsOnly: false,
    bottomPanelHeight: DEFAULT_BOTTOM_PANEL_HEIGHT,
  },
  {
    id: 'compact_laptop_13',
    name: 'Compact Laptop 13"',
    workspacePatch: {
      leftPanelWidth: 260,
      rightPanelWidth: 275,
      filmstripColumns: 1,
      filmstripShowFileInfo: false,
      activePanels: {
        leftTop: Panel.FolderTree,
        leftBottom: null,
        rightTop: Panel.Adjustments,
        rightBottom: null,
      },
    },
    hideCollapsedPanels: true,
    activeToolsOnly: true,
    bottomPanelHeight: 110,
  },
  {
    id: 'culling',
    name: 'Culling',
    workspacePatch: {
      leftPanelWidth: 250,
      rightPanelWidth: 260,
      activePanels: {
        leftTop: Panel.FolderTree,
        leftBottom: null,
        rightTop: Panel.Metadata,
        rightBottom: null,
      },
    },
    hideCollapsedPanels: true,
    activeToolsOnly: false,
    bottomPanelHeight: 168,
    uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: false, quickFilter: true },
  },
  {
    id: 'portrait_retouch',
    name: 'Portrait & Retouch',
    workspacePatch: {
      activePanels: {
        leftTop: Panel.Tones,
        leftBottom: null,
        rightTop: Panel.Masks,
        rightBottom: null,
      },
    },
    hideCollapsedPanels: false,
    activeToolsOnly: true,
  },
  {
    id: 'color_grading',
    name: 'Color Grading',
    workspacePatch: {
      activePanels: {
        leftTop: Panel.Tones,
        leftBottom: null,
        rightTop: Panel.Adjustments,
        rightBottom: null,
      },
    },
    hideCollapsedPanels: false,
    activeToolsOnly: false,
  },
];

export const openMergeModal = (kind: MergeKind, sourcePaths: Array<string>) => {
  useUIStore.getState().setUI({
    mergeModalState: {
      ...CLOSED_MERGE_MODAL_STATE,
      isOpen: true,
      kind,
      settings: DEFAULT_MERGE_SETTINGS[kind],
      sourcePaths,
    },
  });
};

export const closeMergeModal = () => {
  useUIStore.getState().setUI({ mergeModalState: CLOSED_MERGE_MODAL_STATE });
  invoke(Invokes.DiscardMerge).catch((err) => console.error('Failed to discard the merge:', err));
};
