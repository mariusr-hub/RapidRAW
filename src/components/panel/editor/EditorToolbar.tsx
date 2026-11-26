import { memo, useState, useEffect, useRef, useMemo } from 'react';
import {
  ArrowLeft,
  ChevronDown,
  Columns2,
  Eye,
  EyeOff,
  Feather,
  Flame,
  Grid2X2,
  Loader2,
  LucideIcon,
  Maximize,
  Palette,
  Redo,
  RotateCcw,
  Sparkles,
  SquareSplitHorizontal,
  SquareSplitVertical,
  Sun,
  Undo,
  Waves,
  Zap,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { Invokes } from '../../../types/invokes';
import { CompareMode, GroupingMode } from '../../../types/settings';
import { SelectedImage } from '../../../types/editor';
import { ToolbarMenu, ToolbarMenuItem, useMenuDismiss } from './ToolbarMenu';
import { IconAperture, IconCalendar, IconClock, IconFocalLength, IconIso, IconShutter } from './ExifIcons';
import Text from '../../ui/Text';
import { TextColors, TextVariants, TextWeights } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { useLibraryStore } from '../../../store/useLibraryStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { QuickEnhanceAction, useUIStore } from '../../../store/useUIStore';
import { Panel } from '../../../types/ui';
import { useDisplayStatus } from '../../../hooks/useDisplayStatus';
import { findGroupVariants, getVariantLabel } from '../../../utils/imageGrouping';

const COMPARE_MODES = [
  { mode: 'toggle', icon: Eye, label: 'editor.compare.toggle' },
  { mode: 'vertical', icon: SquareSplitHorizontal, label: 'editor.compare.vertical' },
  { mode: 'horizontal', icon: SquareSplitVertical, label: 'editor.compare.horizontal' },
  { mode: 'sideBySide', icon: Columns2, label: 'editor.compare.sideBySide' },
] as const satisfies ReadonlyArray<{ mode: CompareMode; icon: LucideIcon; label: string }>;

interface CompareButtonProps {
  compareMode: CompareMode;
  isComparing: boolean;
  onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void;
  onSelectMode(mode: CompareMode): void;
  onToggle(): void;
}

function HdrButton({ onKeyDown }: { onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void }) {
  const { t } = useTranslation();
  const isEnabled = useSettingsStore((state) => state.appSettings?.hdrDisplay ?? false);

  const handleToggle = () => {
    const { appSettings, handleSettingsChange } = useSettingsStore.getState();
    if (appSettings) {
      handleSettingsChange({ ...appSettings, hdrDisplay: !appSettings.hdrDisplay });
    }
  };

  return (
    <button
      className={clsx(
        'p-2 rounded-full transition-colors',
        isEnabled
          ? 'bg-accent text-button-text hover:bg-accent/90 hover:text-button-text'
          : 'bg-surface hover:bg-card-active text-text-primary',
      )}
      onClick={handleToggle}
      onKeyDown={onKeyDown}
      data-tooltip={t('editor.toolbar.tooltips.hdr')}
    >
      <div className="w-5 h-5 flex items-center justify-center text-[10px] font-bold leading-none tracking-tight">
        HDR
      </div>
    </button>
  );
}

function SoftProofButton({ onKeyDown }: { onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void }) {
  const softProofState = useUIStore((state) => state.softProofState);
  const setUI = useUIStore((state) => state.setUI);
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  useMenuDismiss(menuRef, isMenuOpen, setIsMenuOpen);

  const applyConfig = (patch: Partial<typeof softProofState>) => {
    const next = { ...softProofState, ...patch };
    setUI({ softProofState: next });
    invoke(Invokes.SetSoftProofConfig, { config: next }).catch((err) =>
      console.warn('SetSoftProofConfig failed:', err),
    );
  };

  const handlePickCustomIcc = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'ICC Profiles', extensions: ['icc', 'icm'] }],
      });
      if (typeof selected === 'string') {
        applyConfig({ enabled: true, target: 'custom', customIccPath: selected });
      }
    } catch (err) {
      console.warn('ICC pick failed:', err);
    }
  };

  return (
    <div className="relative flex items-center" ref={menuRef}>
      <button
        type="button"
        className={clsx(
          'p-2 rounded-full transition-colors relative',
          softProofState.enabled
            ? 'bg-accent text-button-text hover:bg-accent/90'
            : 'bg-surface hover:bg-card-active text-text-primary',
        )}
        onClick={() => applyConfig({ enabled: !softProofState.enabled })}
        onContextMenu={(e) => {
          e.preventDefault();
          setIsMenuOpen((prev) => !prev);
        }}
        onKeyDown={onKeyDown}
        data-tooltip="Soft-Proofing (Right-click for Profile / Gamut Options, Shift+G)"
      >
        <div className="relative w-5 h-5 flex items-center justify-center">
          <Palette size={19} />
        </div>
      </button>

      <ToolbarMenu className="w-64 p-2 text-xs" isOpen={isMenuOpen} title="Soft-Proofing">
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-1">
            <span className="text-[10px] uppercase tracking-wider text-text-secondary">Target Space</span>
            <div className="grid grid-cols-2 gap-1">
              {[
                { id: 'srgb', label: 'sRGB' },
                { id: 'displayP3', label: 'Display P3' },
                { id: 'adobeRgb', label: 'Adobe RGB' },
                { id: 'rec2020', label: 'Rec.2020' },
              ].map((tItem) => (
                <button
                  key={tItem.id}
                  type="button"
                  onClick={() => applyConfig({ enabled: true, target: tItem.id, customIccPath: null })}
                  className={clsx(
                    'px-2 py-1 rounded text-left border transition-colors',
                    softProofState.target === tItem.id && !softProofState.customIccPath
                      ? 'bg-accent/20 border-accent text-accent font-medium'
                      : 'bg-surface border-border-color/40 text-text-secondary hover:text-text-primary',
                  )}
                >
                  {tItem.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              onClick={handlePickCustomIcc}
              className="px-2 py-1 rounded text-left border bg-surface border-border-color/40 text-text-secondary hover:text-text-primary truncate"
            >
              {softProofState.customIccPath
                ? `Custom ICC (${softProofState.customIccPath.split(/[/\\]/).pop()})`
                : 'Load Custom ICC/CMYK...'}
            </button>
          </div>

          <div className="flex items-center justify-between gap-1 pt-1 border-t border-white/10">
            <span className="text-[10px] uppercase tracking-wider text-text-secondary">Intent</span>
            <div className="flex rounded bg-surface border border-border-color/40 overflow-hidden">
              {[
                { id: 'relativeColorimetric', label: 'Relative' },
                { id: 'perceptual', label: 'Perceptual' },
              ].map((intent) => (
                <button
                  key={intent.id}
                  type="button"
                  onClick={() => applyConfig({ intent: intent.id })}
                  className={clsx(
                    'px-1.5 py-0.5 text-[10px]',
                    softProofState.intent === intent.id
                      ? 'bg-accent/25 text-accent font-medium'
                      : 'text-text-secondary hover:text-text-primary',
                  )}
                >
                  {intent.label}
                </button>
              ))}
            </div>
          </div>

          <label className="flex items-center justify-between gap-2 pt-1 border-t border-white/10 cursor-pointer">
            <span className="text-[11px] text-text-primary">Out-of-Gamut Overlay</span>
            <input
              type="checkbox"
              checked={softProofState.oogWarning}
              onChange={(e) => applyConfig({ oogWarning: e.target.checked })}
              className="accent-pink-500"
            />
          </label>
        </div>
      </ToolbarMenu>
    </div>
  );
}

function CompareButton({ compareMode, isComparing, onKeyDown, onSelectMode, onToggle }: CompareButtonProps) {
  const { t } = useTranslation();
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const enhancedCompareMode = useEditorStore((state) => state.compareMode);
  const setEditor = useEditorStore((state) => state.setEditor);
  useMenuDismiss(containerRef, isMenuOpen, setIsMenuOpen);

  const ModeIcon =
    compareMode === 'toggle' && isComparing ? EyeOff : COMPARE_MODES.find((m) => m.mode === compareMode)!.icon;

  return (
    <div className="relative" ref={containerRef}>
      <button
        className={clsx(
          'p-2 rounded-full transition-colors',
          isComparing || enhancedCompareMode === 'vertical_split' || enhancedCompareMode === 'horizontal_split'
            ? 'bg-accent text-button-text hover:bg-accent/90 hover:text-button-text'
            : 'bg-surface hover:bg-card-active text-text-primary',
        )}
        onClick={onToggle}
        onKeyDown={onKeyDown}
        onContextMenu={(e) => {
          e.preventDefault();
          setIsMenuOpen((prev) => !prev);
        }}
        data-tooltip={t('editor.toolbar.tooltips.compare')}
      >
        <ModeIcon size={20} />
      </button>

      <ToolbarMenu isOpen={isMenuOpen} title={t('editor.compare.title')}>
        {COMPARE_MODES.map(({ mode, icon, label }) => (
          <ToolbarMenuItem
            key={mode}
            icon={icon}
            isSelected={mode === compareMode && enhancedCompareMode === 'none'}
            label={t(label)}
            onClick={() => {
              setEditor({ compareMode: 'none' });
              onSelectMode(mode);
              setIsMenuOpen(false);
            }}
            onKeyDown={onKeyDown}
          />
        ))}
        <ToolbarMenuItem
          icon={SquareSplitHorizontal}
          isSelected={enhancedCompareMode === 'vertical_split'}
          label="Zoom-Synced Split (Vertical)"
          onClick={() => {
            const next = enhancedCompareMode === 'vertical_split' ? 'none' : 'vertical_split';
            setEditor(
              next === 'none'
                ? { compareMode: 'none' }
                : { compareMode: next, compareView: 'off', showOriginal: false, previewOverride: null },
            );
            setIsMenuOpen(false);
          }}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={SquareSplitVertical}
          isSelected={enhancedCompareMode === 'horizontal_split'}
          label="Zoom-Synced Split (Horizontal)"
          onClick={() => {
            const next = enhancedCompareMode === 'horizontal_split' ? 'none' : 'horizontal_split';
            setEditor(
              next === 'none'
                ? { compareMode: 'none' }
                : { compareMode: next, compareView: 'off', showOriginal: false, previewOverride: null },
            );
            setIsMenuOpen(false);
          }}
          onKeyDown={onKeyDown}
        />
      </ToolbarMenu>
    </div>
  );
}

function QuickEnhanceButton({ onKeyDown }: { onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void }) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  useMenuDismiss(containerRef, isMenuOpen, setIsMenuOpen);

  const dispatchQuick = (panel: Panel, action: QuickEnhanceAction) => {
    useUIStore.getState().triggerQuickAction(panel, action);
    setIsMenuOpen(false);
  };

  return (
    <div className="relative" ref={containerRef}>
      <div className="flex items-center bg-surface rounded-full border border-border-color/40 overflow-hidden">
        <button
          type="button"
          className="hover:bg-card-active text-text-primary pl-2.5 pr-1.5 py-1.5 text-xs font-medium transition-colors flex items-center gap-1.5 cursor-pointer"
          onClick={() => dispatchQuick(Panel.Adjustments, 'smart-tone')}
          onKeyDown={onKeyDown}
          data-tooltip="Smart Tone (Dynamic Exposure & Contrast)"
        >
          <Sun size={14} className="text-amber-400" />
          <span className="hidden xl:inline">Smart Tone</span>
        </button>
        <button
          type="button"
          className="hover:bg-card-active text-text-secondary hover:text-text-primary px-1.5 py-1.5 border-l border-border-color/40 transition-colors cursor-pointer"
          onClick={() => setIsMenuOpen((prev) => !prev)}
          onKeyDown={onKeyDown}
          data-tooltip="More 1-Click Quick Enhancements"
        >
          <ChevronDown size={12} className={clsx('transition-transform', isMenuOpen && 'rotate-180')} />
        </button>
      </div>
      <ToolbarMenu isOpen={isMenuOpen} title="Quick Enhance">
        <ToolbarMenuItem
          icon={Sun}
          isSelected={false}
          label="Smart Tone"
          onClick={() => dispatchQuick(Panel.Adjustments, 'smart-tone')}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={Flame}
          isSelected={false}
          label="Color Contrast"
          onClick={() => dispatchQuick(Panel.Adjustments, 'color-contrast')}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={Feather}
          isSelected={false}
          label="Light Diffusion"
          onClick={() => dispatchQuick(Panel.Adjustments, 'diffusion')}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={Zap}
          isSelected={false}
          label="Tonal Dynamics"
          onClick={() => dispatchQuick(Panel.Adjustments, 'supercontrast')}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={Sparkles}
          isSelected={false}
          label="1-Click Retouch"
          onClick={() => dispatchQuick(Panel.Ai, 'retouch')}
          onKeyDown={onKeyDown}
        />
        <ToolbarMenuItem
          icon={Waves}
          isSelected={false}
          label="Liquify Mesh"
          onClick={() => dispatchQuick(Panel.Ai, 'liquify')}
          onKeyDown={onKeyDown}
        />
      </ToolbarMenu>
    </div>
  );
}

interface EditorToolbarProps {
  canRedo: boolean;
  canUndo: boolean;
  isAndroid: boolean;
  isLoading: boolean;
  onBackToLibrary(): void;
  onImageSelect?(path: string, event?: any): void;
  onRedo(): void;
  onToggleFullScreen(): void;
  onToggleCompare(): void;
  onSelectCompareMode(mode: CompareMode): void;
  onUndo(): void;
  selectedImage: SelectedImage;
  isComparing: boolean;
  compareMode: CompareMode;
  showDateView: boolean;
  onToggleDateView(): void;
  adjustmentsHistoryLabels: string[];
  adjustmentsHistoryIndex: number;
  goToAdjustmentsHistoryIndex(index: number): void;
}

const EditorToolbar = memo(
  ({
    canRedo,
    canUndo,
    isAndroid,
    isLoading,
    onBackToLibrary,
    onImageSelect,
    onRedo,
    onToggleFullScreen,
    onToggleCompare,
    onSelectCompareMode,
    onUndo,
    selectedImage,
    isComparing,
    compareMode,
    showDateView,
    onToggleDateView,
    adjustmentsHistoryLabels,
    adjustmentsHistoryIndex,
    goToAdjustmentsHistoryIndex,
  }: EditorToolbarProps) => {
    const { t } = useTranslation();
    const isAnyLoading = isLoading;
    const [isLoaderVisible, setIsLoaderVisible] = useState(false);
    const [isLoaderMounted, setIsLoaderMounted] = useState(false);
    const [disableLoaderTransition, setDisableLoaderTransition] = useState(false);
    const hideTimeoutRef = useRef<number | null>(null);
    const prevIsLoadingRef = useRef(isLoading);
    const [isVcHovered, setIsVcHovered] = useState(false);
    const [isInfoHovered, setIsInfoHovered] = useState(false);
    const [isHistoryVisible, setIsHistoryVisible] = useState(false);
    const historyButtonRef = useRef<HTMLDivElement>(null);
    useMenuDismiss(historyButtonRef, isHistoryVisible, setIsHistoryVisible);

    const showResolution = !isAndroid && selectedImage.width > 0 && selectedImage.height > 0;
    const [displayedResolution, setDisplayedResolution] = useState('');

    const imageList = useLibraryStore((s) => s.imageList);
    const groupingMode: GroupingMode = useSettingsStore((s) => s.appSettings?.grouping) ?? 'off';

    const variantOptions = useMemo(() => {
      if (groupingMode === 'off' || !onImageSelect) return [];
      const isVC = selectedImage.path.includes('?vc=');
      if (isVC) return [];
      const variants = findGroupVariants(imageList, selectedImage.group_id);
      if (variants.length < 2) return [];
      return variants.map((v) => ({ path: v.path, label: getVariantLabel(v.path) }));
    }, [groupingMode, selectedImage.path, imageList, onImageSelect]);

    const { baseName, isVirtualCopy, vcId, exifData, hasExif } = useMemo(() => {
      const path = selectedImage.path;
      const parts = path.split('?vc=');
      const fullFileName = parts[0].split(/[\\/]/).pop() || '';

      const exif = selectedImage.exif || {};

      let fNum = exif.FNumber;
      if (fNum) {
        const fStr = String(fNum);
        fNum = fStr.toLowerCase().startsWith('f') ? fStr : `f/${fStr}`;
      }

      let captureDate = null;
      let captureTime = null;

      if (exif.DateTimeOriginal) {
        const dateTimeParts = exif.DateTimeOriginal.split(' ');
        captureDate = dateTimeParts[0]?.replace(/:/g, '-') || null;
        if (dateTimeParts[1]) {
          const timeParts = dateTimeParts[1].split(':');
          captureTime = `${timeParts[0]}:${timeParts[1]}`;
        }
      }

      const data = {
        iso: exif.PhotographicSensitivity || exif.ISO,
        fNumber: fNum,
        shutter: exif.ExposureTime,
        focal: exif.FocalLengthIn35mmFilm,
        captureDate: captureDate,
        captureTime: captureTime,
      };

      const hasData = !!(data.iso || data.fNumber || data.shutter || data.focal || data.captureDate);

      return {
        baseName: fullFileName,
        isVirtualCopy: parts.length > 1,
        vcId: parts.length > 1 ? parts[1] : null,
        exifData: data,
        hasExif: hasData,
      };
    }, [selectedImage.path, selectedImage.exif]);

    useEffect(() => {
      if (showResolution) {
        setDisplayedResolution(` - ${selectedImage.width} × ${selectedImage.height}`);
      }
    }, [showResolution, selectedImage.width, selectedImage.height]);

    useEffect(() => {
      const wasLoadingResolution = prevIsLoadingRef.current && !isLoading;

      if (isAnyLoading) {
        if (hideTimeoutRef.current) clearTimeout(hideTimeoutRef.current);
        setDisableLoaderTransition(false);
        setIsLoaderVisible(true);
      } else if (isLoaderVisible) {
        if (wasLoadingResolution) {
          setDisableLoaderTransition(true);
          setIsLoaderVisible(false);
        } else {
          setDisableLoaderTransition(false);
          hideTimeoutRef.current = window.setTimeout(() => {
            setIsLoaderVisible(false);
          }, 300);
        }
      }

      prevIsLoadingRef.current = isLoading;

      return () => {
        if (hideTimeoutRef.current) clearTimeout(hideTimeoutRef.current);
      };
    }, [isAnyLoading, isLoading, isLoaderVisible]);

    useEffect(() => {
      if (isLoaderVisible) {
        setIsLoaderMounted(true);
      } else if (disableLoaderTransition) {
        setIsLoaderMounted(false);
      }
    }, [isLoaderVisible, disableLoaderTransition]);

    useEffect(() => {
      if (isHistoryVisible) {
        const timer = setTimeout(() => {
          const activeEl = historyButtonRef.current?.querySelector('[data-active="true"]');
          if (activeEl) {
            activeEl.scrollIntoView({ block: 'nearest', behavior: 'auto' });
          }
        }, 10);
        return () => clearTimeout(timer);
      }
    }, [isHistoryVisible, adjustmentsHistoryIndex]);

    const isHdrAvailable = useDisplayStatus()?.hdrActive ?? false;

    const handleButtonKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
      if (e.key === 'Tab') return;
      e.currentTarget.blur();
    };

    const isExpanded = isInfoHovered && (hasExif || isLoading);

    return (
      <div className="relative shrink-0 flex items-center justify-between px-3 h-12 gap-3 z-40">
        <div className="flex items-center gap-2 shrink-0 z-40">
          <button
            className="bg-surface text-text-primary p-2 rounded-full hover:bg-card-active transition-colors shrink-0"
            onClick={onBackToLibrary}
            onKeyDown={handleButtonKeyDown}
            data-tooltip={t('editor.toolbar.tooltips.backToLibrary')}
            data-bench-id="back-to-library"
          >
            <ArrowLeft size={20} />
          </button>

          <div className="hidden 2xl:flex items-center gap-2" aria-hidden="true">
            <div className="p-2 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-2 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-2 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-2 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            {isHdrAvailable && (
              <div className="p-2 invisible pointer-events-none">
                <Undo size={20} />
              </div>
            )}
          </div>
        </div>

        <div className="flex-1 flex justify-center min-w-0 relative h-full">
          <div
            className={clsx(
              'bg-surface flex flex-col items-center overflow-hidden transition-all duration-200 ease-out pt-2',
              isExpanded
                ? 'h-18 px-8 rounded-2xl absolute min-w-85 whitespace-nowrap shadow-2xl shadow-black/50'
                : 'h-9 px-4 rounded-[18px] absolute min-w-0 w-auto max-w-full shadow-none',
            )}
            onMouseEnter={() => setIsInfoHovered(true)}
            onMouseLeave={() => setIsInfoHovered(false)}
            style={{
              top: '6px',
              transform: 'translateX(-50%)',
              left: '50%',
              zIndex: isExpanded ? 50 : 0,
            }}
          >
            <div className="flex items-center justify-center max-w-full h-5 shrink-0">
              <Text
                as="span"
                variant={TextVariants.small}
                color={TextColors.primary}
                weight={TextWeights.medium}
                className="truncate min-w-0 shrink"
              >
                {baseName}
              </Text>

              {isVirtualCopy && (
                <Text
                  as="div"
                  variant={TextVariants.small}
                  color={TextColors.accent}
                  weight={TextWeights.bold}
                  className="ml-2 shrink-0 bg-accent/20 px-2 py-0.5 rounded-full flex items-center overflow-hidden cursor-default"
                  onMouseEnter={() => setIsVcHovered(true)}
                  onMouseLeave={() => setIsVcHovered(false)}
                >
                  <span>{t('editor.toolbar.vc')}</span>
                  <div
                    className={clsx(
                      'transition-all duration-300 ease-out overflow-hidden whitespace-nowrap',
                      isVcHovered ? 'max-w-20 opacity-100' : 'max-w-0 opacity-0',
                    )}
                  >
                    <span>-{vcId}</span>
                  </div>
                </Text>
              )}

              {variantOptions.length > 0 && (
                <div className="flex items-center ml-2 shrink-0 border border-text-secondary/20 rounded-full overflow-hidden">
                  {variantOptions.map((v) => {
                    const isActive = v.path === selectedImage.path;
                    return (
                      <button
                        key={v.path}
                        disabled={isActive}
                        className={clsx(
                          'px-2.5 py-1 text-[11px] font-medium transition-colors',
                          isActive ? 'bg-surface text-text-primary' : 'text-text-secondary hover:bg-surface/50',
                        )}
                        data-tooltip={t('editor.toolbar.switchToVariant', { label: v.label })}
                        onClick={(e) => onImageSelect?.(v.path, e)}
                        onKeyDown={handleButtonKeyDown}
                      >
                        {v.label}
                      </button>
                    );
                  })}
                </div>
              )}

              <div
                className={clsx(
                  'transition-all duration-300 ease-out overflow-hidden whitespace-nowrap shrink-0',
                  showResolution ? 'max-w-40 opacity-100 ml-2' : 'max-w-0 opacity-0 ml-0',
                )}
              >
                <Text
                  as="span"
                  variant={TextVariants.small}
                  className={clsx(
                    'block transition-transform duration-200 delay-100',
                    showResolution ? 'scale-100' : 'scale-95',
                  )}
                >
                  {displayedResolution}
                </Text>
              </div>

              <div
                className={clsx(
                  'overflow-hidden shrink-0',
                  isLoaderVisible ? 'max-w-4 opacity-100 ml-2' : 'max-w-0 opacity-0 ml-0',
                  disableLoaderTransition ? 'transition-none' : 'transition-all duration-300',
                )}
                onTransitionEnd={(e) => {
                  if (e.propertyName === 'opacity' && !isLoaderVisible) {
                    setIsLoaderMounted(false);
                  }
                }}
              >
                {isLoaderMounted && <Loader2 size={12} className="text-text-secondary animate-spin" />}
              </div>
            </div>

            <div
              className={clsx(
                'relative mt-2 w-full grow justify-center border-t border-text-secondary/10 pt-2 transition-opacity duration-200',
                isExpanded ? 'opacity-100 delay-75' : 'opacity-0 hidden',
                hasExif && 'cursor-pointer',
              )}
              onClick={() => hasExif && onToggleDateView()}
            >
              <div
                className={clsx(
                  'absolute inset-0 flex items-center justify-center gap-6 transition-opacity duration-200',
                  showDateView ? 'opacity-0 pointer-events-none' : 'opacity-100',
                )}
              >
                {exifData.shutter && (
                  <div className="flex items-center gap-1.5" data-tooltip={t('editor.toolbar.tooltips.shutterSpeed')}>
                    <Text as="span">
                      <IconShutter />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {exifData.shutter}
                    </Text>
                  </div>
                )}
                {exifData.fNumber && (
                  <div className="flex items-center gap-1.5" data-tooltip={t('editor.toolbar.tooltips.aperture')}>
                    <Text as="span">
                      <IconAperture />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {exifData.fNumber}
                    </Text>
                  </div>
                )}
                {exifData.iso && (
                  <div className="flex items-center gap-1.5" data-tooltip={t('editor.toolbar.tooltips.iso')}>
                    <Text as="span">
                      <IconIso />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {exifData.iso}
                    </Text>
                  </div>
                )}
                {exifData.focal && (
                  <div className="flex items-center gap-1.5" data-tooltip={t('editor.toolbar.tooltips.focalLength')}>
                    <Text as="span">
                      <IconFocalLength />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {String(exifData.focal).endsWith('mm') ? exifData.focal : `${exifData.focal}mm`}
                    </Text>
                  </div>
                )}
              </div>

              <div
                className={clsx(
                  'absolute inset-0 flex items-center justify-center gap-6 transition-opacity duration-200',
                  showDateView ? 'opacity-100' : 'opacity-0 pointer-events-none',
                )}
              >
                {exifData.captureDate && (
                  <div className="flex items-center gap-2">
                    <Text as="span">
                      <IconCalendar />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {exifData.captureDate}
                    </Text>
                  </div>
                )}
                {exifData.captureTime && (
                  <div className="flex items-center gap-2">
                    <Text as="span">
                      <IconClock />
                    </Text>
                    <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.medium}>
                      {exifData.captureTime}
                    </Text>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-1 xl:gap-2 shrink-0 z-40">
          <div className="relative flex items-center gap-1 xl:gap-2" ref={historyButtonRef}>
            <button
              className="bg-surface text-text-primary p-1.5 xl:p-2 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!canUndo}
              onClick={onUndo}
              onKeyDown={handleButtonKeyDown}
              onContextMenu={(e) => {
                e.preventDefault();
                setIsHistoryVisible((prev) => !prev);
              }}
              data-tooltip={t('editor.toolbar.tooltips.undo')}
              data-bench-id="undo"
            >
              <Undo size={20} />
            </button>
            <button
              className="bg-surface text-text-primary p-1.5 xl:p-2 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!canRedo}
              onClick={onRedo}
              onKeyDown={handleButtonKeyDown}
              onContextMenu={(e) => {
                e.preventDefault();
                setIsHistoryVisible((prev) => !prev);
              }}
              data-tooltip={t('editor.toolbar.tooltips.redo')}
            >
              <Redo size={20} />
            </button>

            <ToolbarMenu
              className="w-96 max-w-[calc(100vw-1.5rem)]"
              isOpen={isHistoryVisible && adjustmentsHistoryLabels.length > 1}
              title={t('editor.toolbar.history')}
            >
              {adjustmentsHistoryLabels.map((label, i) => (
                <ToolbarMenuItem
                  key={i}
                  detail={i > 0 ? i : undefined}
                  isDimmed={i > adjustmentsHistoryIndex}
                  isSelected={i === adjustmentsHistoryIndex}
                  label={label || 'Initial State'}
                  onClick={() => goToAdjustmentsHistoryIndex(i)}
                  onKeyDown={handleButtonKeyDown}
                  tooltip={label}
                  wrap
                />
              ))}
            </ToolbarMenu>
          </div>

          <QuickEnhanceButton onKeyDown={handleButtonKeyDown} />

          <CompareButton
            compareMode={compareMode}
            isComparing={isComparing}
            onKeyDown={handleButtonKeyDown}
            onSelectMode={onSelectCompareMode}
            onToggle={onToggleCompare}
          />
          <SplitCompareToggle onKeyDown={handleButtonKeyDown} />
          <MatrixCompareToggle onKeyDown={handleButtonKeyDown} />
          <SoftProofButton onKeyDown={handleButtonKeyDown} />
          {isHdrAvailable && <HdrButton onKeyDown={handleButtonKeyDown} />}
          <button
            className="bg-surface text-text-primary p-1.5 xl:p-2 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed relative"
            onClick={onToggleFullScreen}
            onKeyDown={handleButtonKeyDown}
            data-tooltip={t('editor.toolbar.tooltips.fullscreen')}
          >
            <div className="relative w-5 h-5 flex items-center justify-center">
              <Maximize size={20} />
            </div>
          </button>
        </div>
      </div>
    );
  },
);

function SplitCompareToggle({ onKeyDown }: { onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void }) {
  const { t } = useTranslation();
  const compareMode = useEditorStore((state) => state.compareMode);
  const pendingAutoEdit = useEditorStore((state) => state.pendingAutoEdit);
  const selectedImagePath = useEditorStore((state) => state.selectedImage?.path);
  const setEditor = useEditorStore((state) => state.setEditor);
  const isSplit = compareMode === 'vertical_split' || compareMode === 'horizontal_split';

  const handleQuickRejectAuto = () => {
    if (!pendingAutoEdit) return;
    const prev = pendingAutoEdit.previousAdjustments;
    const scene = pendingAutoEdit.scene;
    useEditorStore.getState().pushHistory(prev, 'Reject Auto Changes');
    setEditor({
      adjustments: prev,
      pendingAutoEdit: null,
      compareMode: 'none',
    });
    invoke(Invokes.RecordAutoEditRejection, {
      path: selectedImagePath ?? null,
      scene,
    }).catch((err) => console.warn('RecordAutoEditRejection failed:', err));
  };

  return (
    <>
      <button
        type="button"
        className={clsx(
          'p-2 rounded-full transition-colors',
          isSplit
            ? 'bg-accent text-button-text hover:bg-accent/90'
            : 'bg-surface hover:bg-card-active text-text-primary',
        )}
        onClick={() =>
          setEditor(
            isSplit
              ? { compareMode: 'none' }
              : { compareMode: 'vertical_split', compareView: 'off', showOriginal: false, previewOverride: null },
          )
        }
        onKeyDown={onKeyDown}
        data-tooltip={t('editor.toolbar.tooltips.splitCompare', 'Before / After Split Slider')}
      >
        <SquareSplitHorizontal size={20} />
      </button>
      {pendingAutoEdit && (
        <button
          type="button"
          className="p-2 rounded-full bg-red-500/15 hover:bg-red-500/25 text-red-400 border border-red-500/40 transition-colors"
          onClick={handleQuickRejectAuto}
          onKeyDown={onKeyDown}
          data-tooltip="Reject Auto Changes (1-Click Revert)"
        >
          <RotateCcw size={18} />
        </button>
      )}
    </>
  );
}

function MatrixCompareToggle({ onKeyDown }: { onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>): void }) {
  const compareMode = useEditorStore((state) => state.compareMode);
  const setEditor = useEditorStore((state) => state.setEditor);
  const isActive = compareMode === 'matrix_4way';

  return (
    <button
      className={clsx(
        'p-2 rounded-full transition-colors',
        isActive ? 'bg-accent text-button-text hover:bg-accent/90' : 'bg-surface hover:bg-card-active text-text-primary',
      )}
      onClick={() => setEditor({ compareMode: isActive ? 'none' : 'matrix_4way' })}
      onKeyDown={onKeyDown}
      data-tooltip="4-Way Variant Matrix (A/B/C/D)"
    >
      <Grid2X2 size={20} />
    </button>
  );
}

export default EditorToolbar;
