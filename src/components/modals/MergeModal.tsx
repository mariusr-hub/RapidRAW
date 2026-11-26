import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import {
  CheckCircle,
  Columns,
  Compass,
  Cylinder,
  Expand,
  Eye,
  Flame,
  Globe,
  Images,
  Layers,
  Loader2,
  LucideIcon,
  Map as MapIcon,
  Save,
  ShieldAlert,
  ShieldCheck,
  Square,
  SquaresUnite,
  Sun,
  TriangleAlert,
  WandSparkles,
  XCircle,
  ZoomIn,
} from 'lucide-react';
import { Invokes } from '../../types/invokes';
import { useUIStore } from '../../store/useUIStore';
import { AnimatePresence, motion } from 'framer-motion';
import clsx from 'clsx';
import Button from '../ui/Button';
import Slider from '../ui/Slider';
import Switch from '../ui/Switch';
import Text from '../ui/Text';
import HdrPanoPreflightModal from '../../enhanced/modals/HdrPanoPreflightModal';
import { TextColors, TextVariants } from '../../types/typography';
import {
  FocusStackSettings,
  HdrDeghostMode,
  HdrDeghostSensitivity,
  HdrSettings,
  HdrToneProfile,
  MergeKind,
  MergeOutputFormat,
  MergeSettings,
  PanoramaProjectionMode,
  PanoramaSettings,
} from '../../types/library';
import { MergeModalState } from '../../store/useUIStore';

interface BracketHealthReport {
  is_perfect: boolean;
  aperture_match: boolean;
  highlight_safe: boolean;
  motion_level: number;
  status_badge: string;
  status_message: string;
  dynamic_range_ev: number;
}

const HDR_PROFILE_OPTIONS: ReadonlyArray<{ value: HdrToneProfile; label: string }> = [
  { value: 'natural', label: 'Natural' },
  { value: 'interior', label: 'Interior' },
  { value: 'dramatic', label: 'Dramatic' },
  { value: 'vivid', label: 'Vivid' },
  { value: 'localLaplacian', label: 'Local Laplacian' },
  { value: 'agxScene', label: 'Linear AgX' },
];

const DEGHOST_OPTIONS: ReadonlyArray<{ value: HdrDeghostSensitivity; label: string }> = [
  { value: 'off', label: 'Off' },
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
];

const DEGHOST_MODE_OPTIONS: ReadonlyArray<{ value: HdrDeghostMode; label: string }> = [
  { value: 'opticalFlow', label: 'Optical Flow' },
  { value: 'fast', label: 'Fast (Radiance)' },
];

const SETTINGS_COMMIT_DELAY_MS = 250;

const TOOL_ICONS: Record<MergeKind, { icon: LucideIcon; overlayIcon?: LucideIcon }> = {
  panorama: { icon: SquaresUnite },
  hdr: { icon: Images, overlayIcon: Eye },
  focusStack: { icon: Layers, overlayIcon: MapIcon },
};

interface ToolText {
  description: string;
  failed: string;
  overlay?: string;
  processing: string;
  savedSuccess: string;
  title: string;
}

function useToolText(kind: MergeKind, count: number): ToolText {
  const { t } = useTranslation();
  switch (kind) {
    case 'hdr':
      return {
        description: count ? t('modals.hdr.descriptionWithCount', { count }) : t('modals.hdr.description'),
        failed: t('modals.hdr.failed'),
        overlay: t('modals.hdr.ghostOverlay', 'Ghost Mask'),
        processing: t('modals.hdr.processing'),
        savedSuccess: t('modals.hdr.savedSuccess'),
        title: t('modals.hdr.title'),
      };
    case 'focusStack':
      return {
        description: count
          ? t('modals.focusStack.descriptionWithCount', { count })
          : t('modals.focusStack.description'),
        failed: t('modals.focusStack.failed'),
        overlay: t('modals.focusStack.depthMap'),
        processing: t('modals.focusStack.processing'),
        savedSuccess: t('modals.focusStack.savedSuccess'),
        title: t('modals.focusStack.title'),
      };
    default:
      return {
        description: count ? t('modals.panorama.descriptionWithCount', { count }) : t('modals.panorama.description'),
        failed: t('modals.panorama.failed'),
        processing: t('modals.panorama.processing'),
        savedSuccess: t('modals.panorama.savedSuccess'),
        title: t('modals.panorama.title'),
      };
  }
}

const PROJECTION_OPTIONS = [
  { value: 'auto', label: 'modals.panorama.projectionAuto', fallback: 'Auto', icon: WandSparkles },
  { value: 'spherical', label: 'modals.panorama.projectionSpherical', fallback: 'Spherical', icon: Globe },
  { value: 'cylindrical', label: 'modals.panorama.projectionCylindrical', fallback: 'Cylindrical', icon: Cylinder },
  { value: 'perspective', label: 'modals.panorama.projectionPerspective', fallback: 'Perspective', icon: Square },
  { value: 'panini', label: 'modals.panorama.projectionPanini', fallback: 'Panini (155°)', icon: Expand },
  { value: 'stereographic', label: 'modals.panorama.projectionStereographic', fallback: 'Stereographic', icon: Compass },
] as const;

const METHOD_OPTIONS = [
  { value: 'detail', label: 'modals.focusStack.methodDetail' },
  { value: 'smooth', label: 'modals.focusStack.methodSmooth' },
] as const;

interface SegmentedOption<T extends string> {
  disabled?: boolean;
  icon?: LucideIcon;
  label: string;
  title?: string;
  value: T;
}

interface SegmentedProps<T extends string> {
  cols?: 2 | 3 | 4;
  disabled?: boolean;
  onChange(value: T): void;
  options: ReadonlyArray<SegmentedOption<T>>;
  value: T;
}

function Segmented<T extends string>({ cols = 2, disabled = false, onChange, options, value }: SegmentedProps<T>) {
  const gridCols = cols === 4 ? 'grid-cols-2 sm:grid-cols-4' : cols === 3 ? 'grid-cols-2 sm:grid-cols-3' : 'grid-cols-2';
  return (
    <div className={clsx('grid gap-1.5', gridCols)}>
      {options.map(({ disabled: optDisabled, value: optionValue, label, title, icon: Icon }) => {
        const isDisabled = disabled || optDisabled;
        return (
          <button
            key={optionValue}
            type="button"
            disabled={isDisabled}
            title={title}
            onClick={() => !isDisabled && onChange(optionValue)}
            className={clsx(
              'flex items-center justify-center gap-1 px-2 py-1 text-xs leading-tight rounded-md transition-colors cursor-pointer truncate',
              value === optionValue
                ? 'bg-accent text-button-text font-medium'
                : clsx('bg-bg-primary', !isDisabled && 'hover:bg-card-active'),
              isDisabled && 'opacity-40 cursor-not-allowed',
            )}
          >
            {Icon && <Icon size={13} className="shrink-0" />}
            <span className="truncate">{label}</span>
          </button>
        );
      })}
    </div>
  );
}

interface MergeModalProps {
  loadingImageUrl: string | null;
  onClose(): void;
  onOpenFile(path: string): void;
  onSave(): Promise<string>;
  onSettingsChange(settings: MergeSettings): void;
  onStart(): void;
  state: MergeModalState;
}

export default function MergeModal({
  loadingImageUrl,
  onClose,
  onOpenFile,
  onSave,
  onSettingsChange,
  onStart,
  state,
}: MergeModalProps) {
  const { t } = useTranslation();
  const { error, isOpen, isProcessing, isUpdatingPreview, kind, preview, progressMessage, settings, sourcePaths } =
    state;
  const tool = TOOL_ICONS[kind] ?? TOOL_ICONS.panorama;
  const text = useToolText(kind, sourcePaths.length);
  const [isMounted, setIsMounted] = useState(false);
  const [show, setShow] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [showOverlay, setShowOverlay] = useState(false);
  const [hdrViewMode, setHdrViewMode] = useState<'sdr' | 'hdr10' | 'headroom'>('sdr');
  const [showOrientationControls, setShowOrientationControls] = useState(false);
  const [showSplit, setShowSplit] = useState(false);
  const [splitPos, setSplitPos] = useState(50);
  const [isDraggingSplit, setIsDraggingSplit] = useState(false);
  const [loupeEnabled, setLoupeEnabled] = useState(false);
  const [loupe, setLoupe] = useState<{
    active: boolean;
    boxX: number;
    boxY: number;
    imgX: number;
    imgY: number;
    imgW: number;
    imgH: number;
  }>({
    active: false,
    boxX: 0,
    boxY: 0,
    imgX: 0,
    imgY: 0,
    imgW: 0,
    imgH: 0,
  });
  const [sessionHalfSize, setSessionHalfSize] = useState(false);
  const [showHorizonGrid, setShowHorizonGrid] = useState(false);
  const [showPreflight, setShowPreflight] = useState(false);
  const [healthReport, setHealthReport] = useState<BracketHealthReport | null>(null);
  const [draftSettings, setDraftSettings] = useState<MergeSettings>(settings);
  const [bracketInfo, setBracketInfo] = useState<{
    isHdrBracketSet: boolean;
    panelCount: number;
    bracketSize: number;
    summary: string;
  } | null>(null);

  const mouseDownTarget = useRef<EventTarget | null>(null);
  const previewContainerRef = useRef<HTMLDivElement | null>(null);
  const previewImageWrapRef = useRef<HTMLDivElement | null>(null);
  const horizonDragRef = useRef<{
    active: boolean;
    startX: number;
    startY: number;
    startYaw: number;
    startPitch: number;
  } | null>(null);
  const commitTimer = useRef<number | null>(null);
  const pendingSettings = useRef<MergeSettings | null>(null);
  const projectionNote = useRef<string | null>(null);
  const hasAutoStartedRef = useRef(false);
  const wasCancelledRef = useRef(false);

  useEffect(() => {
    setDraftSettings(settings);
  }, [settings]);

  useEffect(() => {
    if (
      preview?.isSceneReferred === false &&
      (draftSettings.outputFormat === 'dng' || draftSettings.outputFormat === 'ultrahdr')
    ) {
      const next = { ...draftSettings, outputFormat: 'tiff' as MergeOutputFormat };
      setDraftSettings(next);
      useUIStore.getState().setUI((s) => ({
        mergeModalState: { ...s.mergeModalState, settings: next },
      }));
    }
  }, [preview?.isSceneReferred, draftSettings]);

  useEffect(() => {
    if (!isOpen) {
      hasAutoStartedRef.current = false;
      wasCancelledRef.current = false;
      return;
    }
    if (
      !preview &&
      !isProcessing &&
      !error &&
      sourcePaths.length >= 2 &&
      !hasAutoStartedRef.current &&
      !wasCancelledRef.current
    ) {
      hasAutoStartedRef.current = true;
      setSessionHalfSize(Boolean(settings.halfSize ?? (settings as PanoramaSettings).fastDraft));
      onStart();
    }
  }, [isOpen, preview, isProcessing, error, sourcePaths.length, settings, onStart]);

  useEffect(() => {
    if (!isOpen || kind !== 'hdr' || sourcePaths.length < 2) {
      setHealthReport(null);
      return;
    }
    let active = true;
    invoke<BracketHealthReport>(Invokes.ValidateHdrBrackets, { paths: sourcePaths })
      .then((res) => {
        if (active && res) setHealthReport(res);
      })
      .catch((err) => {
        console.warn('HDR bracket validation failed:', err);
        if (active) setHealthReport(null);
      });
    return () => {
      active = false;
    };
  }, [isOpen, kind, sourcePaths]);

  useEffect(() => {
    if (!isOpen || kind !== 'panorama' || sourcePaths.length < 2) {
      setBracketInfo(null);
      return;
    }
    let active = true;
    invoke<{
      isHdrBracketSet: boolean;
      panelCount: number;
      bracketSize: number;
      summary: string;
    }>(Invokes.InspectPanoramaBrackets, { paths: sourcePaths })
      .then((res) => {
        if (active && res) setBracketInfo(res);
      })
      .catch((err) => {
        console.warn('Panorama bracket inspection failed:', err);
        if (active) setBracketInfo(null);
      });
    return () => {
      active = false;
    };
  }, [isOpen, kind, sourcePaths]);

  useEffect(() => {
    if (isOpen) {
      setIsMounted(true);
      const timer = setTimeout(() => setShow(true), 10);
      return () => clearTimeout(timer);
    }
    setShow(false);
    const timer = setTimeout(() => {
      setIsMounted(false);
      setSavedPath(null);
      setIsSaving(false);
      setShowOverlay(false);
      setShowSplit(false);
      setLoupeEnabled(false);
      setSessionHalfSize(false);
      setShowHorizonGrid(false);
    }, 300);
    return () => clearTimeout(timer);
  }, [isOpen]);

  useEffect(
    () => () => {
      if (commitTimer.current !== null) {
        window.clearTimeout(commitTimer.current);
      }
    },
    [],
  );

  const flushSettings = useCallback(() => {
    if (commitTimer.current !== null) {
      window.clearTimeout(commitTimer.current);
      commitTimer.current = null;
    }
    if (pendingSettings.current) {
      onSettingsChange(pendingSettings.current);
      pendingSettings.current = null;
    }
  }, [onSettingsChange]);

  const updateSettings = (patch: Partial<MergeSettings>, requiresRebuild = false) => {
    const next = { ...draftSettings, ...patch } as MergeSettings;
    setDraftSettings(next);
    if (requiresRebuild && preview && !isProcessing) {
      if (commitTimer.current !== null) {
        window.clearTimeout(commitTimer.current);
        commitTimer.current = null;
      }
      pendingSettings.current = null;
      useUIStore.getState().setUI((s) => ({
        mergeModalState: { ...s.mergeModalState, settings: next },
      }));
      setSessionHalfSize(Boolean(next.halfSize ?? (next as PanoramaSettings).fastDraft));
      onStart();
      return;
    }
    pendingSettings.current = next;
    if (commitTimer.current !== null) {
      window.clearTimeout(commitTimer.current);
    }
    commitTimer.current = window.setTimeout(flushSettings, SETTINGS_COMMIT_DELAY_MS);
  };

  const handleClose = useCallback(() => {
    if (isSaving) return;
    if (commitTimer.current !== null) {
      window.clearTimeout(commitTimer.current);
      commitTimer.current = null;
    }
    pendingSettings.current = null;
    onClose();
  }, [onClose, isSaving]);

  const handleBackdropMouseDown = (e: React.MouseEvent) => {
    mouseDownTarget.current = e.target;
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget && mouseDownTarget.current === e.currentTarget) {
      handleClose();
    }
    mouseDownTarget.current = null;
  };

  const handleStart = useCallback(() => {
    const active = pendingSettings.current ?? draftSettings;
    setSessionHalfSize(Boolean(active.halfSize ?? (active as PanoramaSettings).fastDraft));
    flushSettings();
    onStart();
  }, [draftSettings, flushSettings, onStart]);

  const handleCancelProcessing = useCallback(() => {
    wasCancelledRef.current = true;
    invoke(Invokes.DiscardMerge).catch((err) => console.warn('Cancel merge failed:', err));
    useUIStore.getState().setUI((state) => ({
      mergeModalState: {
        ...state.mergeModalState,
        isProcessing: false,
        progressMessage: null,
        error: null,
      },
    }));
  }, []);

  const handleSave = async () => {
    flushSettings();
    setIsSaving(true);
    try {
      const path = await onSave();
      setSavedPath(path);
    } catch (e) {
      console.error(e);
    } finally {
      setIsSaving(false);
    }
  };

  const handleOpen = () => {
    if (savedPath) {
      onOpenFile(savedPath);
      handleClose();
    }
  };

  const renderAutoCrop = (autoCrop: boolean) => (
    <Switch
      label={t('modals.merge.autoCrop')}
      checked={autoCrop}
      onChange={(checked) => updateSettings({ autoCrop: checked })}
      tooltip={t('modals.merge.autoCropHint')}
    />
  );

  const renderPanoramaOptions = (panorama: PanoramaSettings) => {
    const projectionLabel = (value: PanoramaProjectionMode) => {
      const opt = PROJECTION_OPTIONS.find((option) => option.value === value);
      return opt ? t(opt.label, opt.fallback) : t('modals.panorama.projectionAuto', 'Auto');
    };
    if (!isUpdatingPreview) {
      const requested = (settings as PanoramaSettings).projection;
      let note: string | null = preview?.projectionNote ?? null;
      if (!note && preview?.projection && requested === 'auto') {
        note = t('modals.panorama.autoProjectionUsed', { projection: projectionLabel(preview.projection) });
      } else if (!note && preview?.projection && requested !== preview.projection) {
        note = t('modals.panorama.projectionFallback', {
          requested: projectionLabel(requested),
          projection: projectionLabel(preview.projection),
        });
      }
      projectionNote.current = note;
    }
    const hasOrientationOffset =
      (panorama.yawDeg ?? 0) !== 0 || (panorama.pitchDeg ?? 0) !== 0 || (panorama.rollDeg ?? 0) !== 0;
    const isOrientationOpen = showOrientationControls || hasOrientationOffset;
    return [
      <div key="projection">
        <Text variant={TextVariants.heading} className="mb-1.5">
          {t('modals.panorama.projection')}
        </Text>
        <Segmented
          cols={3}
          options={PROJECTION_OPTIONS.map((option) => ({ ...option, label: t(option.label, option.fallback) }))}
          value={panorama.projection}
          onChange={(projection) => updateSettings({ projection })}
        />
        {projectionNote.current && (
          <Text variant={TextVariants.small} className="mt-1.5">
            {projectionNote.current}
          </Text>
        )}
      </div>,
      <div key="boundaryWarp" className="space-y-2.5">
        <Slider
          label={t('modals.panorama.boundaryWarp')}
          min={0}
          max={100}
          step={1}
          defaultValue={0}
          value={panorama.boundaryWarp}
          onChange={(e) => updateSettings({ boundaryWarp: Number(e.target.value) })}
          fillOrigin="min"
        />
        <Text variant={TextVariants.small}>
          {t('modals.panorama.boundaryWarpHint')}
        </Text>
        <div className="pt-2 border-t border-border-color/40 space-y-2">
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() => setShowOrientationControls((v) => !v)}
              className="text-left text-xs font-medium text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
            >
              Horizon &amp; Orientation {isOrientationOpen ? '▾' : '▸'}
            </button>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => setShowHorizonGrid((g) => !g)}
                className={clsx(
                  'px-2 py-0.5 rounded text-[11px] font-medium border transition-colors cursor-pointer',
                  showHorizonGrid
                    ? 'bg-accent text-button-text border-accent'
                    : 'bg-bg-primary border-border-color/50 text-text-secondary hover:text-text-primary',
                )}
                title="Toggle horizon leveling grid & drag-to-level in preview"
              >
                Grid
              </button>
              {hasOrientationOffset && (
                <button
                  type="button"
                  onClick={() => updateSettings({ yawDeg: 0, pitchDeg: 0, rollDeg: 0 })}
                  className="px-2 py-0.5 rounded text-[11px] font-medium bg-bg-primary border border-border-color/50 text-text-secondary hover:text-text-primary cursor-pointer"
                >
                  Reset
                </button>
              )}
            </div>
          </div>
          {isOrientationOpen && (
            <div className="space-y-2">
              <Slider
                label="Roll (Level)"
                min={-45}
                max={45}
                step={0.5}
                defaultValue={0}
                value={panorama.rollDeg ?? 0}
                onChange={(e) => updateSettings({ rollDeg: Number(e.target.value) })}
                suffix="°"
              />
              <Slider
                label="Pitch (Horizon)"
                min={-45}
                max={45}
                step={0.5}
                defaultValue={0}
                value={panorama.pitchDeg ?? 0}
                onChange={(e) => updateSettings({ pitchDeg: Number(e.target.value) })}
                suffix="°"
              />
              <Slider
                label="Yaw (Pan Center)"
                min={-180}
                max={180}
                step={1}
                defaultValue={0}
                value={panorama.yawDeg ?? 0}
                onChange={(e) => updateSettings({ yawDeg: Number(e.target.value) })}
                suffix="°"
              />
            </div>
          )}
        </div>
      </div>,
      <div key="toggles" className="space-y-2.5">
        {renderAutoCrop(panorama.autoCrop)}
        <Switch
          label={t('modals.panorama.exposureCompensation')}
          checked={panorama.exposureCompensation}
          onChange={(exposureCompensation) => updateSettings({ exposureCompensation })}
          tooltip={t('modals.panorama.exposureCompensationHint')}
        />
        {bracketInfo?.isHdrBracketSet && (
          <div className="p-2.5 rounded-lg bg-accent/10 border border-accent/25 space-y-2">
            <Switch
              label={t('modals.panorama.hdrFusion', 'HDR Multi-Bracket Fusion')}
              checked={panorama.hdrPanorama ?? true}
              onChange={(hdrPanorama) => updateSettings({ hdrPanorama }, true)}
              tooltip="Fuse each bracketed exposure stack into 32-bit HDR panels before stitching."
            />
            {(panorama.hdrPanorama ?? true) && (
              <div className="space-y-2 pt-1">
                <div>
                  <Text variant={TextVariants.small} className="mb-1 font-medium text-text-secondary">
                    {t('modals.hdr.visualIntent', 'Visual Intent')}
                  </Text>
                  <Segmented
                    cols={3}
                    options={HDR_PROFILE_OPTIONS}
                    value={panorama.hdrProfile ?? 'natural'}
                    onChange={(hdrProfile) => updateSettings({ hdrProfile })}
                  />
                </div>
                {(panorama.hdrProfile ?? 'natural') !== 'agxScene' && (
                  <Slider
                    label="HDR Tone Strength"
                    min={0}
                    max={100}
                    step={1}
                    defaultValue={100}
                    value={panorama.hdrStrength ?? 100}
                    onChange={(e) => updateSettings({ hdrStrength: Number(e.target.value) })}
                    fillOrigin="min"
                  />
                )}
                <Slider
                  label={t('modals.hdr.exposureBias', 'Exposure Bias (EV)')}
                  min={-2}
                  max={2}
                  step={0.05}
                  defaultValue={0}
                  value={panorama.exposureBias ?? 0}
                  onChange={(e) => updateSettings({ exposureBias: Number(e.target.value) })}
                />
                {(panorama.hdrProfile ?? 'natural') !== 'agxScene' && (
                  <>
                    <Slider
                      label={t('modals.hdr.highlightRecovery', 'Highlight Recovery')}
                      min={0}
                      max={100}
                      step={1}
                      defaultValue={50}
                      value={panorama.highlightRecovery ?? 50}
                      onChange={(e) => updateSettings({ highlightRecovery: Number(e.target.value) })}
                      fillOrigin="min"
                    />
                    <Slider
                      label={t('modals.hdr.shadowLift', 'Shadow Lift')}
                      min={0}
                      max={100}
                      step={1}
                      defaultValue={30}
                      value={panorama.shadowLift ?? 30}
                      onChange={(e) => updateSettings({ shadowLift: Number(e.target.value) })}
                      fillOrigin="min"
                    />
                    <Slider
                      label={t('modals.hdr.microDetail', 'Micro-Detail')}
                      min={0.7}
                      max={1.6}
                      step={0.05}
                      defaultValue={1.05}
                      value={panorama.detailBoost ?? 1.05}
                      onChange={(e) => updateSettings({ detailBoost: Number(e.target.value) })}
                      fillOrigin="min"
                    />
                  </>
                )}
                <button
                  type="button"
                  onClick={() => setShowPreflight(true)}
                  className="w-full text-[11px] font-medium text-accent hover:underline text-left cursor-pointer"
                >
                  Inspect Bracket Groups ({bracketInfo.panelCount} × {bracketInfo.bracketSize})...
                </button>
              </div>
            )}
          </div>
        )}
        {renderSharedOutputOptions(panorama)}
      </div>,
    ];
  };

  const renderHdrOptions = (hdr: HdrSettings) => {
    const profile: HdrToneProfile = hdr.profile ?? 'natural';
    const deghost: HdrDeghostSensitivity = hdr.deghostSensitivity ?? 'medium';
    const isPerceptual = profile !== 'agxScene';
    const isNonRawHdr = kind === 'hdr' && preview?.isSceneReferred === false;
    return [
      <div key="intent" className="space-y-2.5">
        <div>
          <Text variant={TextVariants.heading} className="mb-1.5">
            {t('modals.hdr.visualIntent', 'Visual Intent')}
          </Text>
          <Segmented
            cols={3}
            options={HDR_PROFILE_OPTIONS}
            value={profile}
            onChange={(nextProfile) => updateSettings({ profile: nextProfile })}
          />
        </div>
        {isPerceptual && (
          <Slider
            label={t('modals.hdr.strength', 'HDR Strength')}
            min={0}
            max={100}
            step={1}
            defaultValue={100}
            value={hdr.hdrStrength ?? 100}
            onChange={(e) => updateSettings({ hdrStrength: Number(e.target.value) })}
            fillOrigin="min"
          />
        )}
        <div>
          <Text variant={TextVariants.small} className="mb-1 font-medium text-text-secondary">
            {t('modals.hdr.deghosting', 'Motion Deghosting')}
          </Text>
          <Segmented
            cols={4}
            disabled={isNonRawHdr}
            options={DEGHOST_OPTIONS}
            value={isNonRawHdr ? 'off' : deghost}
            onChange={(deghostSensitivity) => updateSettings({ deghostSensitivity })}
          />
          {!isNonRawHdr && deghost !== 'off' && (
            <div className="mt-2">
              <Text variant={TextVariants.small} className="mb-1 font-medium text-text-secondary">
                {t('modals.hdr.deghostEngine', 'Deghost Engine')}
              </Text>
              <Segmented
                cols={2}
                options={DEGHOST_MODE_OPTIONS}
                value={hdr.deghostMode ?? 'opticalFlow'}
                onChange={(deghostMode) => updateSettings({ deghostMode })}
              />
            </div>
          )}
          {isNonRawHdr && (
            <Text variant={TextVariants.small} className="mt-1 opacity-70">
              Motion Deghosting requires linear RAW brackets (JPEG/TIFF stacks use Mertens exposure fusion).
            </Text>
          )}
        </div>
      </div>,
      <div key="tone" className="space-y-2.5">
        <Text variant={TextVariants.heading} className="mb-0.5">
          {t('modals.hdr.toneControls', 'Tone & Detail')}
        </Text>
        <Slider
          label={t('modals.hdr.exposureBias', 'Exposure Bias (EV)')}
          min={-2}
          max={2}
          step={0.05}
          defaultValue={0}
          value={hdr.exposureBias ?? 0}
          onChange={(e) => updateSettings({ exposureBias: Number(e.target.value) })}
        />
        {isPerceptual && (
          <>
            <Slider
              label={t('modals.hdr.highlightRecovery', 'Highlight Recovery')}
              min={0}
              max={100}
              step={1}
              defaultValue={50}
              value={hdr.highlightRecovery ?? 50}
              onChange={(e) => updateSettings({ highlightRecovery: Number(e.target.value) })}
              fillOrigin="min"
            />
            <Slider
              label={t('modals.hdr.shadowLift', 'Shadow Lift')}
              min={0}
              max={100}
              step={1}
              defaultValue={30}
              value={hdr.shadowLift ?? 30}
              onChange={(e) => updateSettings({ shadowLift: Number(e.target.value) })}
              fillOrigin="min"
            />
            <Slider
              label={t('modals.hdr.microDetail', 'Micro-Detail')}
              min={0.7}
              max={1.6}
              step={0.05}
              defaultValue={1.05}
              value={hdr.detailBoost ?? 1.05}
              onChange={(e) => updateSettings({ detailBoost: Number(e.target.value) })}
              fillOrigin="min"
            />
          </>
        )}
      </div>,
      <div key="toggles" className="space-y-2.5">
        {renderAutoCrop(hdr.autoCrop)}
        {renderSharedOutputOptions(hdr)}
      </div>,
    ];
  };

  const renderSharedOutputOptions = (current: MergeSettings) => {
    const isHalf = !!(current.halfSize ?? (current as PanoramaSettings).fastDraft);
    const applyEdits = (current as HdrSettings | PanoramaSettings).applyEdits ?? true;
    const format: MergeOutputFormat = current.outputFormat ?? 'tiff';
    const isNonRawSession = preview?.isSceneReferred === false;
    return (
      <div className="space-y-2.5">
        {kind !== 'focusStack' && (
          <Switch
            label={t('modals.merge.applySourceEdits', 'Apply Source Edits')}
            checked={applyEdits}
            onChange={(checked) => updateSettings({ applyEdits: checked } as Partial<MergeSettings>, true)}
            tooltip={t(
              'modals.merge.applySourceEditsHint',
              'Applies optical dust heal, defringe, and astro pre-processing on RAW merges, and full .rrdata edits on JPEG/TIFF merges.',
            )}
          />
        )}
        <Switch
          label={t('modals.merge.fastHalfSize', 'Fast Half-Size')}
          checked={isHalf}
          onChange={(checked) =>
            updateSettings(
              kind === 'panorama'
                ? { halfSize: checked, fastDraft: checked }
                : { halfSize: checked },
              true,
            )
          }
          tooltip={t(
            'modals.merge.fastHalfSizeHint',
            'Decodes & renders at half resolution (~4x faster and much smaller output files).',
          )}
        />
        <div>
          <Text variant={TextVariants.small} className="mb-1 font-medium text-text-secondary">
            {t('modals.merge.outputFormat', 'Output Format')}
          </Text>
          <Segmented
            cols={3}
            options={[
              { value: 'tiff' as MergeOutputFormat, label: 'TIFF 16-bit' },
              { value: 'jpeg' as MergeOutputFormat, label: 'JPEG 95%' },
              ...(kind !== 'focusStack'
                ? [
                    {
                      value: 'ultrahdr' as MergeOutputFormat,
                      label: 'Ultra HDR',
                      disabled: isNonRawSession,
                      title: isNonRawSession
                        ? 'Ultra HDR gain maps require linear scene-referred RAW source files'
                        : undefined,
                    },
                  ]
                : []),
              { value: 'png' as MergeOutputFormat, label: 'PNG 16-bit' },
              {
                value: 'dng' as MergeOutputFormat,
                label: 'DNG RAW',
                disabled: isNonRawSession,
                title: isNonRawSession ? 'Linear DNG export requires RAW source files' : undefined,
              },
            ]}
            value={format}
            onChange={(outputFormat) => updateSettings({ outputFormat })}
          />
        </div>
      </div>
    );
  };

  const renderFocusStackOptions = (focusStack: FocusStackSettings) => [
    <div key="method">
      <Text variant={TextVariants.heading} className="mb-1.5">
        {t('modals.focusStack.method')}
      </Text>
      <Segmented
        options={METHOD_OPTIONS.map((option) => ({ ...option, label: t(option.label) }))}
        value={focusStack.method}
        onChange={(method) => updateSettings({ method })}
      />
      <Text variant={TextVariants.small} className="mt-2">
        {t('modals.focusStack.methodHint')}
      </Text>
    </div>,
    <div key="toggles" className="space-y-2.5">
      {renderAutoCrop(focusStack.autoCrop)}
      {renderSharedOutputOptions(focusStack)}
    </div>,
  ];

  const renderOptions = () => {
    const columns =
      kind === 'panorama'
        ? renderPanoramaOptions(draftSettings as PanoramaSettings)
        : kind === 'hdr'
          ? renderHdrOptions(draftSettings as HdrSettings)
          : renderFocusStackOptions(draftSettings as FocusStackSettings);
    const settingsClass = clsx(isSaving && 'opacity-50 pointer-events-none');
    return (
      <div
        className={clsx(
          'grid grid-cols-1 gap-4 mt-3 pt-3 border-t border-border-color/40',
          columns.length >= 3 ? 'md:grid-cols-3' : 'md:grid-cols-2',
        )}
      >
        {columns.map((column) => (
          <div key={column.key} className={settingsClass}>
            {column}
          </div>
        ))}
      </div>
    );
  };

  const switchHdrViewMode = (nextMode: 'sdr' | 'hdr10' | 'headroom') => {
    setShowOverlay(false);
    setShowSplit(false);
    setHdrViewMode(nextMode);
    if (kind === 'hdr' && preview?.isSceneReferred) {
      updateSettings({ previewMode: nextMode } as Partial<HdrSettings>, true);
    }
  };

  const renderOverlayToggle = () => {
    const hasOverlay = Boolean(preview?.overlayBase64 && text.overlay && tool.overlayIcon);
    const hasSplit = Boolean(kind === 'hdr' && preview?.referenceBase64);
    const hasHdrViews = kind === 'hdr' && Boolean(preview?.isSceneReferred);
    const hasLoupe = kind === 'panorama' || kind === 'hdr';
    if (!hasOverlay && !hasSplit && !hasHdrViews && !hasLoupe) return null;
    const OverlayIcon = tool.overlayIcon;
    const ToolIcon = tool.icon;
    return (
      <div className="flex flex-wrap items-center gap-1 bg-bg-primary/80 border border-border-color/40 p-1 rounded-lg">
        {(hasOverlay || hasSplit || hasHdrViews) && (
          <button
            type="button"
            onClick={() => switchHdrViewMode('sdr')}
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              !showOverlay && !showSplit && hdrViewMode === 'sdr'
                ? 'bg-accent text-button-text'
                : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
          >
            <ToolIcon size={13} />
            <span>{t('modals.merge.result')}</span>
          </button>
        )}
        {hasHdrViews && (
          <button
            type="button"
            onClick={() => switchHdrViewMode('hdr10')}
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              !showOverlay && !showSplit && hdrViewMode === 'hdr10'
                ? 'bg-accent text-button-text'
                : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
            title="Preview linear scene radiance in 16-bit HDR10 (Rec.2020 PQ)"
          >
            <Sun size={13} />
            <span>HDR10</span>
          </button>
        )}
        {hasHdrViews && (
          <button
            type="button"
            onClick={() => switchHdrViewMode('headroom')}
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              !showOverlay && !showSplit && hdrViewMode === 'headroom'
                ? 'bg-accent text-button-text'
                : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
            title="False-color HDR headroom heatmap (+1/+2/+3 EV above 203 nits)"
          >
            <Flame size={13} />
            <span>Headroom</span>
          </button>
        )}
        {hasOverlay && OverlayIcon && (
          <button
            type="button"
            onClick={() => {
              setShowOverlay(true);
              setShowSplit(false);
            }}
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              showOverlay ? 'bg-accent text-button-text' : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
          >
            <OverlayIcon size={13} />
            <span>{text.overlay}</span>
          </button>
        )}
        {hasSplit && (
          <button
            type="button"
            onClick={() => {
              setShowOverlay(false);
              setHdrViewMode('sdr');
              setShowSplit((prev) => {
                const next = !prev;
                if (next) {
                  setLoupeEnabled(false);
                  setLoupe((l) => ({ ...l, active: false }));
                }
                return next;
              });
            }}
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              showSplit ? 'bg-accent text-button-text' : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
          >
            <Columns size={13} />
            <span>Before / After</span>
          </button>
        )}
        {hasLoupe && (
          <button
            type="button"
            onClick={() =>
              setLoupeEnabled((prev) => {
                const next = !prev;
                if (next) {
                  setShowSplit(false);
                } else {
                  setLoupe((l) => ({ ...l, active: false }));
                }
                return next;
              })
            }
            className={clsx(
              'flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors select-none cursor-pointer whitespace-nowrap',
              loupeEnabled ? 'bg-accent text-button-text' : 'text-text-secondary hover:bg-card-active hover:text-text-primary',
            )}
            title="Toggle 2.5x Hover Detail Loupe"
          >
            <ZoomIn size={13} />
            <span>2.5× Loupe</span>
          </button>
        )}
        {hasHdrViews && preview?.hdrPeakNits != null && (
          <span className="px-2 py-0.5 rounded bg-amber-500/15 border border-amber-500/30 text-[11px] font-mono text-amber-300 whitespace-nowrap">
            {Math.round(preview.hdrPeakNits)} nits
          </span>
        )}
      </div>
    );
  };

  const renderContent = () => {
    if (error) {
      return (
        <div className="flex flex-col items-center justify-center py-8 h-[clamp(220px,46vh,380px)]">
          <div className="flex items-center justify-center mb-5">
            <XCircle className="w-12 h-12 text-red-500" />
          </div>
          <Text variant={TextVariants.title} className="mb-2 text-center">
            {text.failed}
          </Text>
          <Text className="text-center p-4 rounded-lg bg-bg-primary max-w-md mt-2 leading-relaxed">
            {String(error)}
          </Text>
        </div>
      );
    }

    if (preview && !isProcessing) {
      const imageSrc =
        showOverlay && preview.overlayBase64
          ? preview.overlayBase64
          : kind === 'hdr' && preview.isSceneReferred && hdrViewMode === 'hdr10' && preview.hdrPreviewBase64
            ? preview.hdrPreviewBase64
            : kind === 'hdr' && preview.isSceneReferred && hdrViewMode === 'headroom' && preview.headroomMapBase64
              ? preview.headroomMapBase64
              : preview.base64;
      const isHalfSelected = Boolean(draftSettings.halfSize ?? (draftSettings as PanoramaSettings).fastDraft);
      const scaleDiv = isHalfSelected && !sessionHalfSize ? 2 : 1;
      const updateSplitFromClientX = (clientX: number) => {
        const el = previewImageWrapRef.current ?? previewContainerRef.current;
        if (!el) return;
        const rect = el.getBoundingClientRect();
        if (rect.width <= 0) return;
        setSplitPos(Math.max(2, Math.min(98, ((clientX - rect.left) / rect.width) * 100)));
      };
      const handlePreviewMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
        if (horizonDragRef.current?.active && kind === 'panorama' && showHorizonGrid && !showSplit && !loupeEnabled) {
          const { startX, startY, startYaw, startPitch } = horizonDragRef.current;
          const nextYaw = Math.max(-180, Math.min(180, Math.round(startYaw + (e.clientX - startX) * 0.25)));
          const nextPitch = Math.max(-45, Math.min(45, Math.round((startPitch + (e.clientY - startY) * 0.15) * 2) / 2));
          updateSettings({ yawDeg: nextYaw, pitchDeg: nextPitch });
          return;
        }
        if (isDraggingSplit && showSplit) {
          updateSplitFromClientX(e.clientX);
        }
        if (loupeEnabled && previewImageWrapRef.current && previewContainerRef.current) {
          const imgRect = previewImageWrapRef.current.getBoundingClientRect();
          const boxRect = previewContainerRef.current.getBoundingClientRect();
          const imgX = e.clientX - imgRect.left;
          const imgY = e.clientY - imgRect.top;
          if (imgX < 0 || imgX > imgRect.width || imgY < 0 || imgY > imgRect.height || imgRect.width <= 0 || imgRect.height <= 0) {
            setLoupe((prev) => (prev.active ? { ...prev, active: false } : prev));
            return;
          }
          setLoupe({
            active: true,
            boxX: e.clientX - boxRect.left,
            boxY: e.clientY - boxRect.top,
            imgX,
            imgY,
            imgW: imgRect.width,
            imgH: imgRect.height,
          });
        }
      };
      return (
        <div className="w-full">
          {kind === 'hdr' && healthReport && (
            <div
              className={clsx(
                'mb-2 px-3 py-1.5 rounded-lg border text-xs flex items-center justify-between gap-2',
                healthReport.is_perfect
                  ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                  : 'bg-amber-500/10 border-amber-500/30 text-amber-300',
              )}
            >
              <span className="flex items-center gap-1.5">
                {healthReport.is_perfect ? <ShieldCheck size={14} /> : <ShieldAlert size={14} />}
                <span>{healthReport.status_message}</span>
              </span>
              <span className="font-mono text-[11px] opacity-80">
                {healthReport.dynamic_range_ev.toFixed(1)} EV
              </span>
            </div>
          )}

          <div
            ref={previewContainerRef}
            onMouseMove={handlePreviewMouseMove}
            onMouseLeave={() => {
              setIsDraggingSplit(false);
              if (horizonDragRef.current) horizonDragRef.current.active = false;
              setLoupe((prev) => ({ ...prev, active: false }));
            }}
            onMouseDown={(e) => {
              if (showSplit) {
                setIsDraggingSplit(true);
                updateSplitFromClientX(e.clientX);
              } else if (kind === 'panorama' && showHorizonGrid && !loupeEnabled) {
                const pano = draftSettings as PanoramaSettings;
                horizonDragRef.current = {
                  active: true,
                  startX: e.clientX,
                  startY: e.clientY,
                  startYaw: pano.yawDeg ?? 0,
                  startPitch: pano.pitchDeg ?? 0,
                };
              }
            }}
            onMouseUp={() => {
              setIsDraggingSplit(false);
              if (horizonDragRef.current) horizonDragRef.current.active = false;
            }}
            className={clsx(
              'group relative w-full h-[clamp(180px,36vh,340px)] bg-[#111] rounded-lg overflow-hidden border border-surface flex items-center justify-center select-none',
              showSplit
                ? 'cursor-ew-resize'
                : kind === 'panorama' && showHorizonGrid && !loupeEnabled
                  ? 'cursor-move'
                  : loupeEnabled
                    ? 'cursor-crosshair'
                    : '',
            )}
          >
            <div
              ref={previewImageWrapRef}
              className="relative inline-flex items-center justify-center max-w-full max-h-full"
            >
              <img
                src={imageSrc}
                alt={text.title}
                className="block max-w-full max-h-[clamp(176px,35vh,336px)] w-auto h-auto object-contain"
                draggable={false}
              />
              {kind === 'panorama' && showHorizonGrid && (
                <svg
                  className="absolute inset-0 w-full h-full pointer-events-none z-10"
                  viewBox="0 0 300 200"
                  preserveAspectRatio="none"
                >
                  <line x1="0" y1="100" x2="300" y2="100" stroke="rgba(255,210,80,0.85)" strokeWidth="1" strokeDasharray="4 3" />
                  <line x1="150" y1="0" x2="150" y2="200" stroke="rgba(255,210,80,0.65)" strokeWidth="1" strokeDasharray="4 3" />
                  <line x1="0" y1="66.7" x2="300" y2="66.7" stroke="rgba(255,255,255,0.28)" strokeWidth="0.6" />
                  <line x1="0" y1="133.3" x2="300" y2="133.3" stroke="rgba(255,255,255,0.28)" strokeWidth="0.6" />
                  <line x1="100" y1="0" x2="100" y2="200" stroke="rgba(255,255,255,0.28)" strokeWidth="0.6" />
                  <line x1="200" y1="0" x2="200" y2="200" stroke="rgba(255,255,255,0.28)" strokeWidth="0.6" />
                </svg>
              )}
              {showSplit && preview.referenceBase64 && (
                <>
                  <div
                    className="absolute inset-0 pointer-events-none"
                    style={{ clipPath: `inset(0 ${100 - splitPos}% 0 0)` }}
                  >
                    <img
                      src={preview.referenceBase64}
                      alt="0 EV Reference"
                      className="block w-full h-full object-contain"
                      draggable={false}
                    />
                  </div>
                  <div
                    className="absolute top-0 bottom-0 w-0.5 bg-white/90 shadow-[0_0_8px_rgba(0,0,0,0.85)] pointer-events-none z-10"
                    style={{ left: `${splitPos}%`, transform: 'translateX(-50%)' }}
                  >
                    <div className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-6 h-6 rounded-full bg-black/80 border border-white/60 flex items-center justify-center text-[10px] text-white">
                      ⇆
                    </div>
                  </div>
                </>
              )}
            </div>
            {showSplit && preview.referenceBase64 && (
              <>
                <div className="absolute top-2.5 left-2.5 px-2 py-0.5 rounded bg-black/70 text-[10px] font-mono text-white/90 pointer-events-none">
                  BEFORE (0 EV)
                </div>
                <div className="absolute top-2.5 right-2.5 px-2 py-0.5 rounded bg-accent/90 text-[10px] font-mono font-semibold text-button-text pointer-events-none">
                  AFTER (HDR)
                </div>
              </>
            )}
            {loupeEnabled && loupe.active && loupe.imgW > 0 && loupe.imgH > 0 && (
              <div
                className="pointer-events-none absolute w-36 h-36 rounded-full border-2 border-accent shadow-2xl overflow-hidden z-20"
                style={{
                  left: Math.max(8, Math.min((previewContainerRef.current?.clientWidth ?? 400) - 152, loupe.boxX - 72)),
                  top: Math.max(8, Math.min((previewContainerRef.current?.clientHeight ?? 360) - 152, loupe.boxY - 72)),
                  backgroundImage: `url(${imageSrc})`,
                  backgroundRepeat: 'no-repeat',
                  backgroundSize: `${loupe.imgW * 2.5}px ${loupe.imgH * 2.5}px`,
                  backgroundPosition: `${-(loupe.imgX * 2.5 - 72)}px ${-(loupe.imgY * 2.5 - 72)}px`,
                }}
              />
            )}
            <AnimatePresence>
              {isUpdatingPreview && (
                <motion.div
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.15 }}
                  className="absolute inset-0 flex items-center justify-center bg-black/30"
                >
                  <Loader2 className="w-8 h-8 animate-spin text-white" />
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <div className="mt-2">{renderOverlayToggle()}</div>

          <div className="flex flex-wrap items-center justify-between gap-2 mt-2">
            <div className="flex flex-wrap items-center gap-2">
              <Text variant={TextVariants.small}>
                {isSaving && progressMessage
                  ? progressMessage
                  : t('modals.merge.outputInfo', {
                      images: preview.imageCount,
                      width: scaleDiv > 1 ? Math.max(1, Math.round(preview.width / scaleDiv)) : preview.width,
                      height: scaleDiv > 1 ? Math.max(1, Math.round(preview.height / scaleDiv)) : preview.height,
                    })}
              </Text>
              {typeof preview.fovDeg === 'number' && (
                <span className="px-2 py-0.5 rounded bg-bg-primary border border-border-color/40 text-[11px] font-mono text-text-secondary">
                  FOV {Math.round(preview.fovDeg)}°
                </span>
              )}
              {kind === 'panorama' && preview.wraps && (
                <span className="px-2 py-0.5 rounded bg-accent/15 border border-accent/40 text-[11px] font-medium text-accent">
                  360° Equirectangular
                </span>
              )}
            </div>
            {(preview.excludedCount > 0 || preview.unalignedCount > 0) && (
              <Text
                as="div"
                variant={TextVariants.small}
                color={TextColors.accent}
                className="flex items-center gap-1.5"
              >
                <TriangleAlert size={14} />
                {preview.excludedCount > 0
                  ? t('modals.merge.excludedImages', { count: preview.excludedCount })
                  : t('modals.merge.unalignedImages', { count: preview.unalignedCount })}
              </Text>
            )}
          </div>

          {savedPath ? (
            <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.3 }}>
              <Text
                as="div"
                variant={TextVariants.heading}
                color={TextColors.success}
                className="flex items-center justify-center gap-2 mt-4"
              >
                <CheckCircle className="w-5 h-5" />
                <span>{text.savedSuccess}</span>
              </Text>
            </motion.div>
          ) : (
            renderOptions()
          )}
        </div>
      );
    }

    if (isProcessing) {
      const pctMatch = progressMessage?.match(/(\d+(?:\.\d+)?)%/);
      const pctValue = pctMatch ? Math.min(100, Math.max(0, parseFloat(pctMatch[1]))) : null;
      return (
        <div className="flex h-[clamp(220px,46vh,380px)] overflow-hidden rounded-lg border border-surface">
          <div className="w-2/5 relative overflow-hidden shrink-0 bg-[#0a0a0a] flex items-center justify-center">
            {loadingImageUrl ? (
              <img src={loadingImageUrl} alt="Source preview" className="w-full h-full object-cover" />
            ) : (
              <div className="w-full h-full bg-surface/50" />
            )}
          </div>
          <div className="flex-1 flex flex-col items-center justify-center px-12 bg-bg-primary">
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.1, duration: 0.4 }}
              className="flex flex-col items-center w-full"
            >
              <Text variant={TextVariants.title} className="mb-2 text-center">
                {text.processing}
              </Text>
              <Text className="text-center font-mono h-6 flex justify-center items-center">
                {progressMessage || t('modals.merge.initializing')}
              </Text>

              <div className="mt-8 w-64 relative">
                {pctValue !== null ? (
                  <div className="space-y-1.5">
                    <div className="h-2 bg-surface rounded-full overflow-hidden w-full shadow-xs border border-border-color/40">
                      <div
                        className="h-full bg-accent transition-all duration-200 rounded-full"
                        style={{ width: `${pctValue}%` }}
                      />
                    </div>
                    <div className="text-center text-[11px] font-mono text-text-secondary">
                      {Math.round(pctValue)}%
                    </div>
                  </div>
                ) : (
                  <div className="h-1 bg-surface rounded-full overflow-hidden relative w-full shadow-xs">
                    <motion.div
                      className="absolute inset-y-0 w-[80%] bg-linear-to-r from-transparent via-accent to-transparent mix-blend-screen"
                      style={{ filter: 'blur(3px)' }}
                      animate={{ x: ['-150%', '150%'] }}
                      transition={{ repeat: Infinity, duration: 1.5, ease: [0.4, 0, 0.2, 1] }}
                    />
                    <motion.div
                      className="absolute inset-y-0 w-[40%] bg-linear-to-r from-transparent via-white/90 to-transparent"
                      style={{ filter: 'blur(1px)' }}
                      animate={{ x: ['-250%', '250%'] }}
                      transition={{ repeat: Infinity, duration: 1.5, ease: [0.4, 0, 0.2, 1] }}
                    />
                  </div>
                )}
              </div>

              <Text variant={TextVariants.small} className="mt-6 text-center max-w-xs opacity-60">
                {t('modals.merge.speedNotice')}
              </Text>
              <Button variant="secondary" onClick={handleCancelProcessing} className="mt-6">
                <XCircle className="mr-1.5" size={16} />
                {t('modals.merge.cancel')}
              </Button>
            </motion.div>
          </div>
        </div>
      );
    }

    const ToolIcon = tool.icon;
    return (
      <div className="w-full">
        <div className="flex flex-col items-center justify-center py-3">
          <div className="flex items-center justify-center mb-2">
            <ToolIcon className="w-10 h-10 text-accent" />
          </div>
          <Text variant={TextVariants.title} className="mb-1 text-center">
            {text.title}
          </Text>
          <Text className="text-center max-w-md text-sm leading-relaxed text-text-secondary">{text.description}</Text>
          {bracketInfo?.isHdrBracketSet && (
            <div className="mt-2.5 px-3 py-1.5 rounded-lg bg-accent/10 border border-accent/30 text-xs text-text-primary flex items-center gap-2 max-w-lg">
              <TriangleAlert size={14} className="text-accent shrink-0" />
              <span>{bracketInfo.summary}</span>
            </div>
          )}
        </div>
        {kind === 'panorama' ? (
          <div className="border-t border-border-color/40 pt-3">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {renderPanoramaOptions(draftSettings as PanoramaSettings).map((col) => (
                <div key={col.key}>{col}</div>
              ))}
            </div>
          </div>
        ) : (
          <div className="border-t border-border-color/40 pt-4 max-w-md mx-auto">
            {renderSharedOutputOptions(draftSettings)}
          </div>
        )}
      </div>
    );
  };

  const renderButtons = () => {
    if (error) {
      return (
        <Button onClick={handleClose} className="w-full">
          {t('modals.merge.close')}
        </Button>
      );
    }

    if (savedPath) {
      return (
        <>
          <button
            onClick={handleClose}
            className="px-4 py-2 rounded-md text-text-secondary hover:bg-card-active transition-colors"
          >
            {t('modals.merge.close')}
          </button>
          <Button onClick={handleOpen}>{t('modals.merge.openInEditor')}</Button>
        </>
      );
    }

    const disabled = isProcessing || isSaving;
    const ToolIcon = tool.icon;

    return (
      <div className="w-full flex items-center justify-end gap-2">
        <button
          onClick={handleClose}
          disabled={isSaving}
          className={clsx(
            'px-4 py-2 rounded-md text-text-secondary hover:bg-card-active transition-colors text-sm',
            isSaving && 'opacity-50 pointer-events-none',
          )}
        >
          {preview ? t('modals.merge.close') : t('modals.merge.cancel')}
        </button>

        {preview ? (
          <Button onClick={handleSave} disabled={isSaving || isProcessing || isUpdatingPreview}>
            {isSaving ? <Loader2 className="animate-spin mr-2" size={16} /> : <Save className="mr-2" size={16} />}
            {isSaving ? t('modals.merge.saving') : t('modals.merge.save')}
          </Button>
        ) : (
          <Button onClick={handleStart} disabled={disabled}>
            {isProcessing ? (
              <Loader2 className="animate-spin mr-2" size={16} />
            ) : (
              <ToolIcon className="mr-2" size={16} />
            )}
            {t('modals.merge.start')}
          </Button>
        )}
      </div>
    );
  };

  if (!isMounted) return null;

  return (
    <div
      className={clsx(
        'fixed inset-0 flex items-center justify-center p-2 sm:p-4 z-50 bg-black/40 backdrop-blur-xs transition-opacity duration-300 ease-in-out',
        show ? 'opacity-100' : 'opacity-0',
      )}
      onMouseDown={handleBackdropMouseDown}
      onClick={handleBackdropClick}
    >
      <div
        className={clsx(
          'bg-surface rounded-xl shadow-2xl p-4 sm:p-5 w-full max-w-5xl max-h-[92vh] flex flex-col overflow-hidden transform transition-all duration-300 ease-out',
          show ? 'scale-100 opacity-100 translate-y-0' : 'scale-95 opacity-0 -translate-y-4',
        )}
        onClick={(e) => e.stopPropagation()}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex-1 min-h-0 overflow-y-auto pr-1">
          {renderContent()}
        </div>
        <div className={clsx('shrink-0 mt-3 flex justify-end gap-3', !savedPath && 'pt-3 border-t border-surface/50')}>
          {renderButtons()}
        </div>
      </div>

      <HdrPanoPreflightModal
        isOpen={showPreflight}
        paths={sourcePaths}
        initialProjection={(() => {
          const p = (draftSettings as PanoramaSettings).projection;
          if (p === 'perspective') return 'planar';
          if (p === 'auto' || !p) return 'cylindrical';
          return p;
        })()}
        initialBoundaryWarp={((draftSettings as PanoramaSettings).boundaryWarp ?? 0) / 100}
        onClose={() => setShowPreflight(false)}
        onProceed={(selectedProj, selectedWarp) => {
          setShowPreflight(false);
          const mappedProjection: PanoramaProjectionMode =
            selectedProj === 'planar' ? 'perspective' : selectedProj;
          updateSettings(
            {
              projection: mappedProjection,
              boundaryWarp: Math.round(selectedWarp * 100),
              hdrPanorama: true,
            },
            true,
          );
        }}
      />
    </div>
  );
}
