import React, { useCallback, useMemo, useState } from 'react';
import {
  RotateCcw,
  Copy,
  ClipboardPaste,
  PencilSparkles,
  ChartArea,
  LayoutList,
  ChevronDown,
  Sparkles,
  Layers,
  Moon,
  DollarSign,
  Wand2,
  Palette,
  Trophy,
  Package,
  Download,
  Zap,
  Telescope,
  Camera,
  Film,
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import CollapsibleSection from '../../ui/CollapsibleSection';
import AdjustmentSection from '../../adjustments/AdjustmentSection';
import ConnectedWaveform from '../editor/ConnectedWaveform';
import Resizer from '../../ui/Resizer';
import AdjustmentSectionsSubMenu from './AdjustmentSectionsSubMenu';
import NightSkyDropzone from '../../../enhanced/panels/NightSkyDropzone';
import FocusStackDropzone from '../../../enhanced/panels/FocusStackDropzone';
import StockPrepDropzone from '../../../enhanced/panels/StockPrepDropzone';
import {
  ActiveTools,
  Adjustments,
  ADJUSTMENT_SECTIONS,
  getActiveTools,
  getVisibleAdjustmentSections,
  isToolActive,
  setToolActive,
  toggleToolActive,
} from '../../../utils/adjustments';
import { useContextMenu } from '../../../context/ContextMenuContext';
import { OPTION_SEPARATOR, Orientation, Panel } from '../../../types/ui';
import Text from '../../ui/Text';
import { TextVariants, TextColors, TextWeights } from '../../../types/typography';
import { useShallow } from 'zustand/react/shallow';
import { canvasToolState, useEditorStore } from '../../../store/useEditorStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { useLibraryStore } from '../../../store/useLibraryStore';
import clsx from 'clsx';
import { getOpenImageDefaults } from '../../../utils/developDefaults';
import { useUIStore } from '../../../store/useUIStore';
import { buildAutoAdjustModeOptions, useEditorActions } from '../../../hooks/useEditorActions';
import { useWaveformControls } from '../../../hooks/useWaveformControls';
import type { ParseKeys } from 'i18next';

function AdjustmentSectionContent(props: {
  sectionName: string;
  activeTools: ActiveTools;
  onToggleTool(id: string): void;
  [key: string]: unknown;
}) {
  const adjustments = useEditorStore((state) => state.adjustments);
  return <AdjustmentSection {...props} adjustments={adjustments} />;
}

export default function Controls() {
  const { t } = useTranslation();
  const { showContextMenu } = useContextMenu();
  const { isResizingWaveform, onToggleWaveform, setActiveWaveformChannel, handleWaveformResize } =
    useWaveformControls();
  const { setAdjustments, handleAutoAdjustments, handleLutSelect } = useEditorActions();

  const { appSettings, theme } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      theme: state.theme,
    })),
  );

  const visibleSections = getVisibleAdjustmentSections(appSettings?.adjustmentLayout);

  const { collapsibleSectionsState, setUI } = useUIStore(
    useShallow((state) => ({
      collapsibleSectionsState: state.collapsibleSectionsState,
      setUI: state.setUI,
    })),
  );

  const {
    activeToolsState,
    sectionVisibility,
    copiedSectionAdjustments,
    selectedImage,
    isWbPickerActive,
    isWaveformVisible,
    waveformHeight,
    setEditor,
  } = useEditorStore(
    useShallow((state) => ({
      activeToolsState: state.adjustments.activeTools,
      sectionVisibility: state.adjustments.sectionVisibility,
      copiedSectionAdjustments: state.copiedSectionAdjustments,
      selectedImage: state.selectedImage,
      isWbPickerActive: state.isWbPickerActive,
      isWaveformVisible: state.isWaveformVisible,
      waveformHeight: state.waveformHeight,
      setEditor: state.setEditor,
    })),
  );

  const activeTools = useMemo(
    () => getActiveTools({ activeTools: activeToolsState, sectionVisibility }),
    [activeToolsState, sectionVisibility],
  );

  const setCopiedSectionAdjustments = useCallback(
    (val: any) => setEditor({ copiedSectionAdjustments: val }),
    [setEditor],
  );

  const toggleWbPicker = useCallback(
    () => setEditor((state) => canvasToolState({ isWbPickerActive: !state.isWbPickerActive })),
    [setEditor],
  );

  const onDragStateChange = useCallback(
    (isDragging: boolean) => setEditor({ isSliderDragging: isDragging }),
    [setEditor],
  );

  const setCollapsibleState = useCallback(
    (updater: any) =>
      setUI((state) => ({
        collapsibleSectionsState: typeof updater === 'function' ? updater(state.collapsibleSectionsState) : updater,
      })),
    [setUI],
  );

  const handleToggleVisibility = useCallback(
    (id: string) => {
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        activeTools: toggleToolActive(getActiveTools(prev), id),
      }));
    },
    [setAdjustments],
  );

  const handleResetAdjustments = async () => {
    const defaults = await getOpenImageDefaults();
    setAdjustments(
      (prev: Adjustments) => ({
        ...prev,
        ...Object.keys(ADJUSTMENT_SECTIONS)
          .flatMap((s) => ADJUSTMENT_SECTIONS[s])
          .reduce((acc: any, key: string) => {
            acc[key] = defaults[key as keyof Adjustments];
            return acc;
          }, {}),
        activeTools: { ...defaults.activeTools },
      }),
      t('contextMenus.editor.resetAdjustments'),
    );
  };

  const handleAutoAdjustContextMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    showContextMenu(event.clientX, event.clientY, buildAutoAdjustModeOptions(t, handleAutoAdjustments));
  };

  const handleToggleSection = (section: string) => {
    setCollapsibleState((prev: any) => {
      const isOpening = !prev[section];
      if (appSettings?.enableFocusMode && isOpening) {
        const newState = { ...prev };
        Object.keys(newState).forEach((key) => {
          newState[key] = false;
        });
        newState[section] = true;
        return newState;
      }
      return { ...prev, [section]: !prev[section] };
    });
  };

  const handleSectionContextMenu = (event: any, sectionName: string) => {
    event.preventDefault();
    event.stopPropagation();

    const sectionKeys = ADJUSTMENT_SECTIONS[sectionName];
    if (!sectionKeys) {
      return;
    }

    const handleCopy = () => {
      const { adjustments } = useEditorStore.getState();
      const adjustmentsToCopy: any = {};
      for (const key of sectionKeys) {
        if (Object.prototype.hasOwnProperty.call(adjustments, key)) {
          adjustmentsToCopy[key] = JSON.parse(JSON.stringify(adjustments[key as keyof Adjustments]));
        }
      }
      setCopiedSectionAdjustments({ section: sectionName, values: adjustmentsToCopy });
    };

    const handlePaste = () => {
      if (!copiedSectionAdjustments || copiedSectionAdjustments.section !== sectionName) {
        return;
      }
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        ...copiedSectionAdjustments.values,
        activeTools: setToolActive(getActiveTools(prev), sectionName, true),
      }));
    };

    const handleReset = async () => {
      const defaults = await getOpenImageDefaults();
      const resetValues: any = {};
      for (const key of sectionKeys) {
        resetValues[key] = defaults[key as keyof Adjustments];
      }
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        ...resetValues,
        activeTools: setToolActive(getActiveTools(prev), sectionName, true),
      }));
    };

    const isPasteAllowed = copiedSectionAdjustments && copiedSectionAdjustments.section === sectionName;
    const translatedSection = t(`editor.adjustments.sections.${sectionName}` as ParseKeys);

    const pasteLabel = copiedSectionAdjustments
      ? t('editor.adjustments.actions.pasteLabel', { section: translatedSection })
      : t('editor.adjustments.actions.pasteSettings');

    const options: any = [
      {
        label: t('editor.adjustments.actions.copySectionSettings', { section: translatedSection }),
        icon: Copy,
        onClick: handleCopy,
      },
      { label: pasteLabel, icon: ClipboardPaste, onClick: handlePaste, disabled: !isPasteAllowed },
      { type: OPTION_SEPARATOR },
      {
        label: t('editor.adjustments.actions.resetSectionSettings', { section: translatedSection }),
        icon: RotateCcw,
        onClick: handleReset,
      },
      { type: OPTION_SEPARATOR },
      {
        label: t('editor.adjustments.actions.customizePanels'),
        icon: LayoutList,
        submenu: [{ customComponent: AdjustmentSectionsSubMenu }],
      },
    ];

    showContextMenu(event.clientX, event.clientY, options);
  };

  return (
    <div className="flex flex-col h-full">
      <div className="p-3 flex justify-between items-center shrink-0 border-b border-surface">
        <Text variant={TextVariants.title}>{t('editor.adjustments.title')}</Text>
        <div className="flex items-center gap-1">
          <button
            className="p-2 rounded-full hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            disabled={!selectedImage}
            onClick={() => handleAutoAdjustments()}
            onContextMenu={handleAutoAdjustContextMenu}
            data-tooltip={t('editor.adjustments.tooltips.autoAdjust')}
          >
            <PencilSparkles size={18} />
          </button>
          <button
            className={clsx(
              'p-2 rounded-full transition-colors',
              isWaveformVisible ? 'bg-surface hover:bg-card-active' : 'hover:bg-surface',
            )}
            onClick={onToggleWaveform}
            data-tooltip={t('editor.adjustments.tooltips.toggleAnalytics')}
          >
            <ChartArea size={18} />
          </button>
          <button
            className="p-2 rounded-full hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            disabled={!selectedImage}
            onClick={handleResetAdjustments}
            data-tooltip={t('editor.adjustments.tooltips.resetAdjustments')}
          >
            <RotateCcw size={18} />
          </button>
        </div>
      </div>

      <AnimatePresence initial={false}>
        {isWaveformVisible && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: waveformHeight || 256, opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: isResizingWaveform ? 0 : 0.2, ease: 'easeOut' }}
            className="shrink-0 flex flex-col relative border-b border-surface overflow-hidden"
          >
            <div className="grow w-full h-full p-3 pb-2 min-h-0">
              <ConnectedWaveform
                setDisplayMode={setActiveWaveformChannel}
                setAdjustments={setAdjustments}
                theme={theme}
              />
            </div>
            <Resizer direction={Orientation.Horizontal} onMouseDown={handleWaveformResize} />
          </motion.div>
        )}
      </AnimatePresence>

      <div className="grow overflow-y-scroll p-3 flex flex-col gap-2">
        {selectedImage ? (
          visibleSections.map((sectionName: string) => {
            const title = t(`editor.adjustments.sections.${sectionName}` as ParseKeys);

            return (
              <div className="shrink-0 group" key={sectionName}>
                <CollapsibleSection
                  isContentVisible={isToolActive(activeTools, sectionName)}
                  isOpen={collapsibleSectionsState[sectionName as keyof typeof collapsibleSectionsState]}
                  onContextMenu={(e: any) => handleSectionContextMenu(e, sectionName)}
                  onToggle={() => handleToggleSection(sectionName)}
                  onToggleVisibility={() => handleToggleVisibility(sectionName)}
                  title={title}
                >
                  <AdjustmentSectionContent
                    sectionName={sectionName}
                    activeTools={activeTools}
                    onToggleTool={handleToggleVisibility}
                    setAdjustments={setAdjustments}
                    theme={theme}
                    handleLutSelect={handleLutSelect}
                    appSettings={appSettings}
                    asShotWhiteBalance={selectedImage.asShotWhiteBalance}
                    isWbPickerActive={isWbPickerActive}
                    toggleWbPicker={toggleWbPicker}
                    onDragStateChange={onDragStateChange}
                  />
                </CollapsibleSection>
              </div>
            );
          })
        ) : (
          <div className="flex items-center justify-center py-8">
            <Text
              variant={TextVariants.heading}
              color={TextColors.secondary}
              weight={TextWeights.normal}
              className="text-center"
            >
              {t('editor.ai.noImageSelected')}
            </Text>
          </div>
        )}

        <EnhancedStudioDrawer selectedImage={selectedImage} setUI={setUI} />
      </div>
    </div>
  );
}

function EnhancedStudioDrawer({
  selectedImage,
  setUI,
}: {
  selectedImage: { path: string; isRaw?: boolean } | null;
  setUI: ReturnType<typeof useUIStore.getState>['setUI'];
}) {
  const [isOpen, setIsOpen] = useState(true);
  const [activeTab, setActiveTab] = useState<'nightSky' | 'focusStack' | 'stockPrep'>('nightSky');
  const multiSelectedPaths = useLibraryStore((s) => s.multiSelectedPaths);
  const studioPaths = useMemo(
    () => (multiSelectedPaths.length > 0 ? multiSelectedPaths : selectedImage ? [selectedImage.path] : []),
    [multiSelectedPaths, selectedImage],
  );

  return (
    <div className="shrink-0 mt-2 rounded-lg border border-border-color/60 bg-bg-secondary/40 overflow-hidden">
      <button
        type="button"
        onClick={() => setIsOpen((prev) => !prev)}
        className="w-full flex items-center justify-between px-3 py-2 text-xs font-semibold text-text-primary hover:bg-card-active/40 transition-colors cursor-pointer"
      >
        <span className="flex items-center gap-1.5">
          <Sparkles size={14} className="text-accent" />
          <span>Enhanced Studio & Dropzones</span>
        </span>
        <ChevronDown
          size={14}
          className={clsx('text-text-secondary transition-transform duration-200', isOpen && 'rotate-180')}
        />
      </button>

      {isOpen && (
        <div className="p-2.5 space-y-3 border-t border-border-color/40">
          <div className="grid grid-cols-2 gap-1.5 text-[11px]">
            <button
              type="button"
              onClick={() => setUI({ batchPolishModalState: { isOpen: true, selectedPaths: studioPaths } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Wand2 size={12} className="text-accent shrink-0" />
              <span className="truncate">AI Batch Polish</span>
            </button>
            <button
              type="button"
              onClick={() =>
                setUI({
                  colorMatcherModalState: {
                    isOpen: true,
                    heroPath: selectedImage?.path ?? studioPaths[0] ?? null,
                    targetPaths: studioPaths,
                    initialMode: 'matcher',
                  },
                })
              }
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Palette size={12} className="text-purple-400 shrink-0" />
              <span className="truncate">Color Matcher</span>
            </button>
            <button
              type="button"
              onClick={() => setUI({ heroCuratorModalState: { isOpen: true, selectedPaths: studioPaths } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Trophy size={12} className="text-amber-400 shrink-0" />
              <span className="truncate">Hero Curator</span>
            </button>
            <button
              type="button"
              onClick={() => setUI({ speedCullerModalState: { isOpen: true, selectedPaths: studioPaths, initialIndex: 0 } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Zap size={12} className="text-yellow-400 shrink-0" />
              <span className="truncate">Speed Culler</span>
            </button>
            <button
              type="button"
              onClick={() => setUI({ clientDeliveryModalState: { isOpen: true, selectedPaths: studioPaths } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Package size={12} className="text-emerald-400 shrink-0" />
              <span className="truncate">Client Delivery</span>
            </button>
            <button
              type="button"
              onClick={() => setUI({ advancedExportModalState: { isOpen: true, selectedPaths: studioPaths } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Download size={12} className="text-sky-400 shrink-0" />
              <span className="truncate">Multi-Export</span>
            </button>
            <button
              type="button"
              onClick={() =>
                setUI((s) => ({
                  drizzleModalState: { ...s.drizzleModalState, isOpen: true, sourcePaths: studioPaths },
                }))
              }
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Telescope size={12} className="text-indigo-400 shrink-0" />
              <span className="truncate">Astro Drizzle</span>
            </button>
            <button
              type="button"
              onClick={() => {
                const ui = useUIStore.getState();
                const hasTethering = Object.values(ui.panelLayout).some((panels) => panels.includes(Panel.Tethering));
                if (hasTethering) {
                  ui.setPanel(Panel.Tethering);
                } else {
                  ui.movePanelToIndex(Panel.Tethering, 'leftTop', ui.panelLayout.leftTop.length);
                  ui.setActivePanel('leftTop', Panel.Tethering);
                }
              }}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer"
            >
              <Camera size={12} className="text-rose-400 shrink-0" />
              <span className="truncate">Tether Studio</span>
            </button>
            <button
              type="button"
              onClick={() => setUI({ negativeModalState: { isOpen: true, targetPaths: studioPaths } })}
              className="flex items-center gap-1.5 px-2 py-1.5 rounded bg-surface hover:bg-card-active text-text-primary cursor-pointer col-span-2"
            >
              <Film size={12} className="text-orange-400 shrink-0" />
              <span className="truncate">Film Negative Converter</span>
            </button>
          </div>

          <div className="flex rounded-md bg-surface/60 p-0.5 text-[11px] gap-0.5">
            <button
              type="button"
              onClick={() => setActiveTab('nightSky')}
              className={clsx(
                'flex-1 flex items-center justify-center gap-1 py-1 rounded transition-colors cursor-pointer',
                activeTab === 'nightSky' ? 'bg-accent text-button-text font-medium' : 'text-text-secondary hover:text-text-primary',
              )}
            >
              <Moon size={11} />
              <span>Astro</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab('focusStack')}
              className={clsx(
                'flex-1 flex items-center justify-center gap-1 py-1 rounded transition-colors cursor-pointer',
                activeTab === 'focusStack' ? 'bg-accent text-button-text font-medium' : 'text-text-secondary hover:text-text-primary',
              )}
            >
              <Layers size={11} />
              <span>Focus</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab('stockPrep')}
              className={clsx(
                'flex-1 flex items-center justify-center gap-1 py-1 rounded transition-colors cursor-pointer',
                activeTab === 'stockPrep' ? 'bg-accent text-button-text font-medium' : 'text-text-secondary hover:text-text-primary',
              )}
            >
              <DollarSign size={11} />
              <span>Stock</span>
            </button>
          </div>

          {activeTab === 'nightSky' && <NightSkyDropzone />}
          {activeTab === 'focusStack' && <FocusStackDropzone />}
          {activeTab === 'stockPrep' && <StockPrepDropzone />}
        </div>
      )}
    </div>
  );
}
