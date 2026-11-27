import { memo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { ChevronDown, Loader2, Sparkles, X } from 'lucide-react';
import clsx from 'clsx';
import { Invokes } from '../../types/invokes';
import Slider, { SliderChangeEvent } from '../ui/Slider';
import Switch from '../ui/Switch';
import Button from '../ui/Button';
import Text from '../ui/Text';
import { TextVariants } from '../../types/typography';
import { SpatialEffect } from './Effects';
import {
  Adjustments,
  DenoiseMethod,
  DetailsAdjustment,
  getAdjustmentToolOrder,
  getAiDenoiseEnablePatch,
  getHiddenAdjustmentTools,
  areSectionPropsEqual,
} from '../../utils/adjustments';
import { AppSettings } from '../../types/settings';
import AdjustmentSubSection from './AdjustmentSubSection';
import { canvasToolState, useEditorStore } from '../../store/useEditorStore';
import { selectAiModelDownloadStatus, useProcessStore } from '../../store/useProcessStore';
import { useUIStore } from '../../store/useUIStore';

interface DetailsPanelProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments>): any;
  appSettings: AppSettings | null;
  isForMask?: boolean;
  onDragStateChange?: (isDragging: boolean) => void;
}

function DetailsPanel({
  adjustments,
  setAdjustments,
  appSettings,
  isForMask = false,
  onDragStateChange,
}: DetailsPanelProps) {
  const { t } = useTranslation();
  const setEditor = useEditorStore((state) => state.setEditor);
  const selectedImage = useEditorStore((state) => state.selectedImage);
  const setUI = useUIStore((state) => state.setUI);
  const [isEdgeOpticsOpen, setIsEdgeOpticsOpen] = useState(false);
  const [isEnhancedPresenceOpen, setIsEnhancedPresenceOpen] = useState(false);
  const aiDenoiseProgress = useEditorStore((state) =>
    state.selectedImage ? state.aiDenoiseProgress[state.selectedImage.path] : undefined,
  );
  const aiDenoiseMeta = useEditorStore((state) =>
    state.selectedImage ? state.aiDenoiseMeta[state.selectedImage.path] : undefined,
  );
  const aiUpscaleProgress = useEditorStore((state) =>
    state.selectedImage ? state.aiUpscaleProgress[state.selectedImage.path] : undefined,
  );
  const aiModelDownloadStatus = useProcessStore(selectAiModelDownloadStatus);

  const handleAdjustmentChange = (key: string, value: string) => {
    const numericValue = parseInt(value, 10);
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: numericValue }));
  };

  const handleFloatAdjustmentChange = (key: string, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: parseFloat(value) }));
  };

  const isAiFree = appSettings?.aiProvider === 'ai-free';
  const denoiseMethod = adjustments.aiDenoiseEnabled
    ? (adjustments.aiDenoiseMethod ?? DenoiseMethod.Ai)
    : isAiFree
      ? DenoiseMethod.Bm3d
      : DenoiseMethod.Ai;
  const hiddenTools = getHiddenAdjustmentTools(appSettings?.adjustmentLayout);
  const toolOrder = getAdjustmentToolOrder('details', appSettings?.adjustmentLayout?.toolOrder);

  const hasEdgeOpticsActive =
    Math.abs(adjustments.microContrast ?? 0) > 1e-4 ||
    (adjustments.cornerSharpnessBoost ?? 0) > 0 ||
    (adjustments.bokehMaskProtection ?? 0) > 0;
  const hasEnhancedPresenceActive =
    (adjustments.clearView ?? 0) > 0 ||
    (adjustments.skinShield ?? 0) > 1e-4 ||
    (adjustments.contrastCleaner ?? 0) > 1e-4;

  return (
    <div className="flex flex-col gap-4">
      {!hiddenTools.includes('sharpening') && (
        <AdjustmentSubSection
          id="sharpening"
          order={toolOrder.indexOf('sharpening')}
          title={t('adjustments.details.sharpening')}
          hasActiveAdjustment={
            adjustments.sharpness !== 0 || (adjustments.sharpnessThreshold ?? 15) !== 15 || hasEdgeOpticsActive
          }
        >
          <Slider
            resetKey="sharpness"
            label={t('adjustments.details.sharpness')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.Sharpness, e.target.value)}
            step={1}
            value={adjustments.sharpness}
            onDragStateChange={onDragStateChange}
          />
          {!isForMask && (
            <>
              <Slider
                resetKey="sharpnessThreshold"
                label={t('adjustments.details.threshold')}
                max={80}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.SharpnessThreshold, e.target.value)}
                step={1}
                value={adjustments.sharpnessThreshold ?? 15}
                onDragStateChange={onDragStateChange}
                defaultValue={15}
                fillOrigin="min"
              />
              <div className="mt-2 rounded-md border border-border-color/60 bg-bg-secondary/40 overflow-hidden">
                <button
                  type="button"
                  onClick={() => setIsEdgeOpticsOpen((prev) => !prev)}
                  className="w-full flex items-center justify-between px-2.5 py-1.5 text-xs font-medium text-text-primary hover:bg-card-active/40 transition-colors cursor-pointer"
                >
                  <span className="flex items-center gap-1.5">
                    <span>{t('adjustments.details.focusMaskOptics', 'FocusMask & Edge Optics')}</span>
                    {hasEdgeOpticsActive && <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />}
                  </span>
                  <ChevronDown
                    size={14}
                    className={clsx(
                      'text-text-secondary transition-transform duration-200',
                      isEdgeOpticsOpen && 'rotate-180',
                    )}
                  />
                </button>
                {isEdgeOpticsOpen && (
                  <div className="px-2 pb-2 pt-1 space-y-1 border-t border-border-color/40">
                    <Slider
                      resetKey="microContrast"
                      label={t('adjustments.details.microContrast', 'Micro-Contrast')}
                      max={100}
                      min={-100}
                      defaultValue={0}
                      onChange={(e: any) => handleAdjustmentChange('microContrast', e.target.value)}
                      step={1}
                      value={adjustments.microContrast ?? 0}
                      onDragStateChange={onDragStateChange}
                    />
                    <Slider
                      resetKey="cornerSharpnessBoost"
                      label={t('adjustments.details.cornerSharpnessBoost', 'Corner Sharpness')}
                      max={100}
                      min={0}
                      defaultValue={0}
                      onChange={(e: any) => handleAdjustmentChange('cornerSharpnessBoost', e.target.value)}
                      step={1}
                      value={adjustments.cornerSharpnessBoost ?? 0}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      resetKey="bokehMaskProtection"
                      label={t('adjustments.details.bokehMaskProtection', 'Bokeh Mask Protection')}
                      max={100}
                      min={0}
                      defaultValue={0}
                      onChange={(e: any) => handleAdjustmentChange('bokehMaskProtection', e.target.value)}
                      step={1}
                      value={adjustments.bokehMaskProtection ?? 0}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </div>
                )}
              </div>
            </>
          )}
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('presence') && (
        <AdjustmentSubSection
          id="presence"
          order={toolOrder.indexOf('presence')}
          title={t('adjustments.details.presence')}
        >
          <Slider
            resetKey="clarity"
            label={t('adjustments.details.clarity')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.Clarity, e.target.value)}
            speedEditId={DetailsAdjustment.Clarity}
            step={1}
            value={adjustments.clarity}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            resetKey="dehaze"
            label={t('adjustments.details.dehaze')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.Dehaze, e.target.value)}
            speedEditId={DetailsAdjustment.Dehaze}
            step={1}
            value={adjustments.dehaze}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            resetKey="structure"
            label={t('adjustments.details.structure')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.Structure, e.target.value)}
            speedEditId={DetailsAdjustment.Structure}
            step={1}
            value={adjustments.structure}
            onDragStateChange={onDragStateChange}
          />
          {!isForMask && (
            <>
              <div
                data-center-marker-controls="centre"
                onPointerDown={() => setEditor(canvasToolState({ activeCenterMarker: 'centre' }))}
              >
                <Slider
                  resetKey="centré"
                  label={t('adjustments.details.centre')}
                  max={100}
                  min={-100}
                  onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.Centré, e.target.value)}
                  step={1}
                  value={adjustments.centré}
                  onDragStateChange={onDragStateChange}
                />
              </div>
              <div className="mt-2 rounded-md border border-border-color/60 bg-bg-secondary/40 overflow-hidden">
                <button
                  type="button"
                  onClick={() => setIsEnhancedPresenceOpen((prev) => !prev)}
                  className="w-full flex items-center justify-between px-2.5 py-1.5 text-xs font-medium text-text-primary hover:bg-card-active/40 transition-colors cursor-pointer"
                >
                  <span className="flex items-center gap-1.5">
                    <span>{t('adjustments.details.enhancedPresence', 'Smart Clarity & Skin Shield')}</span>
                    {hasEnhancedPresenceActive && <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />}
                  </span>
                  <ChevronDown
                    size={14}
                    className={clsx(
                      'text-text-secondary transition-transform duration-200',
                      isEnhancedPresenceOpen && 'rotate-180',
                    )}
                  />
                </button>
                {isEnhancedPresenceOpen && (
                  <div className="px-2 pb-2 pt-1 space-y-1 border-t border-border-color/40">
                    <Slider
                      resetKey="skinShield"
                      label={t('adjustments.details.skinShield', 'Skin Shield')}
                      max={1.5}
                      min={0}
                      defaultValue={0}
                      onChange={(e: any) => handleFloatAdjustmentChange('skinShield', String(e.target.value))}
                      step={0.05}
                      value={adjustments.skinShield ?? 0}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      resetKey="contrastCleaner"
                      speedEditId="contrastCleaner"
                      label={t('adjustments.details.contrastCleaner', 'Halo Contrast Cleaner')}
                      max={2}
                      min={0}
                      defaultValue={0}
                      onChange={(e: any) => handleFloatAdjustmentChange('contrastCleaner', String(e.target.value))}
                      step={0.05}
                      value={adjustments.contrastCleaner ?? 0}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      resetKey="clearView"
                      label={t('adjustments.details.clearView', 'ClearView Haze Inversion')}
                      max={100}
                      min={0}
                      defaultValue={0}
                      onChange={(e: any) => handleAdjustmentChange('clearView', String(e.target.value))}
                      step={1}
                      value={adjustments.clearView ?? 0}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </div>
                )}
              </div>
            </>
          )}
        </AdjustmentSubSection>
      )}

      {!isForMask && !hiddenTools.includes('hlr') && (
        <AdjustmentSubSection
          id="hlr"
          order={toolOrder.indexOf('hlr')}
          title={t('adjustments.details.hlr', 'Highlight Reconstruction')}
          hasActiveAdjustment={adjustments.hlrEnabled === true}
        >
          <div className="space-y-2">
            <Switch
              checked={adjustments.hlrEnabled ?? false}
              label={t('adjustments.details.hlrEnabled', 'Highlight Chroma Recovery')}
              onChange={(checked) =>
                setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, hlrEnabled: checked }))
              }
            />
            {(adjustments.hlrEnabled ?? false) && (
              <Slider
                resetKey="hlrClipCeiling"
                label={t('adjustments.details.hlrClipCeiling', 'Clip Ceiling')}
                max={1.0}
                min={0.5}
                defaultValue={0.95}
                onChange={(e: any) => handleFloatAdjustmentChange('hlrClipCeiling', String(e.target.value))}
                step={0.01}
                value={adjustments.hlrClipCeiling ?? 0.95}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            )}
          </div>
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('noiseReduction') && (
        <AdjustmentSubSection
          id="noiseReduction"
          order={toolOrder.indexOf('noiseReduction')}
          title={t('adjustments.details.noiseReduction')}
        >
          <Slider
            resetKey="lumaNoiseReduction"
            label={t('adjustments.details.luminance')}
            max={100}
            min={isForMask ? -100 : 0}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.LumaNoiseReduction, e.target.value)}
            step={1}
            value={adjustments.lumaNoiseReduction}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            resetKey="colorNoiseReduction"
            label={t('adjustments.details.color')}
            max={100}
            min={isForMask ? -100 : 0}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.ColorNoiseReduction, e.target.value)}
            step={1}
            value={adjustments.colorNoiseReduction}
            onDragStateChange={onDragStateChange}
          />
          {!isForMask && (
            <div className="mt-2">
              <SpatialEffect
                enabled={!!adjustments.aiDenoiseEnabled}
                label={
                  denoiseMethod === DenoiseMethod.Ai
                    ? t('adjustments.details.aiDenoise')
                    : t('adjustments.details.classicDenoise')
                }
                onToggle={(enabled: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({
                    ...prev,
                    ...(enabled ? getAiDenoiseEnablePatch(appSettings?.aiProvider) : { aiDenoiseEnabled: false }),
                  }))
                }
              >
                {aiDenoiseProgress !== undefined ? (
                  <div className="flex flex-col items-center justify-center gap-1.5 p-4 text-text-secondary text-center">
                    <div className="flex items-center gap-2">
                      <Loader2 size={16} className="animate-spin shrink-0" />
                      <Text variant={TextVariants.label}>
                        {aiModelDownloadStatus
                          ? t('editor.masks.settings.aiModelDownloading')
                          : aiDenoiseMeta?.stage || t('editor.ai.denoising')}
                      </Text>
                    </div>
                    <Text variant={TextVariants.small} className="text-accent font-mono">
                      {aiModelDownloadStatus ||
                        `${Math.round(aiDenoiseProgress * 100)}%${
                          aiDenoiseMeta?.tileIndex && aiDenoiseMeta?.totalTiles
                            ? ` · Tile ${aiDenoiseMeta.tileIndex}/${aiDenoiseMeta.totalTiles}`
                            : ''
                        }${aiDenoiseMeta?.etaSec !== undefined ? ` · ~${aiDenoiseMeta.etaSec}s` : ''}`}
                    </Text>
                    {aiDenoiseMeta?.provider && !aiModelDownloadStatus && (
                      <Text variant={TextVariants.small} className="text-text-secondary/80 font-mono text-[11px]">
                        Backend: {aiDenoiseMeta.provider}
                      </Text>
                    )}
                    <button
                      type="button"
                      onClick={() => {
                        invoke(Invokes.CancelAiDenoise).catch((err) => console.warn('Cancel failed:', err));
                        setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, aiDenoiseEnabled: false }));
                      }}
                      className="mt-1 inline-flex items-center gap-1 px-2.5 py-1 rounded text-[11px] bg-surface border border-border-color/60 hover:bg-card-active text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
                    >
                      <X size={12} />
                      <span>Cancel</span>
                    </button>
                  </div>
                ) : (
                  <>
                    <Slider
                      resetKey="aiDenoiseAmount"
                      label={t('adjustments.effects.amount')}
                      max={100}
                      min={0}
                      defaultValue={100}
                      onChange={(e: SliderChangeEvent) =>
                        handleAdjustmentChange(DetailsAdjustment.AiDenoiseAmount, String(e.target.value))
                      }
                      step={1}
                      value={adjustments.aiDenoiseAmount ?? 100}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      resetKey="aiDenoiseDetail"
                      label={t('adjustments.details.aiDenoiseDetail')}
                      max={100}
                      min={0}
                      onChange={(e: SliderChangeEvent) =>
                        handleAdjustmentChange(DetailsAdjustment.AiDenoiseDetail, String(e.target.value))
                      }
                      step={1}
                      value={adjustments.aiDenoiseDetail ?? 15}
                      defaultValue={15}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </>
                )}
              </SpatialEffect>
              {selectedImage && (
                <Button
                  variant="secondary"
                  className="mt-2 w-full flex items-center justify-center gap-1.5 text-xs py-1.5"
                  onClick={() =>
                    setUI({
                      denoiseModalState: {
                        isOpen: true,
                        isProcessing: false,
                        previewBase64: null,
                        originalBase64: null,
                        error: null,
                        targetPaths: [selectedImage.path],
                        progressMessage: null,
                        isRaw: selectedImage.isRaw,
                      },
                    })
                  }
                >
                  <Sparkles size={14} className="text-accent" />
                  {t('adjustments.details.openDenoiseStudio', 'Open AI Denoise Studio...')}
                </Button>
              )}
            </div>
          )}
        </AdjustmentSubSection>
      )}

      {!isForMask && !isAiFree && !hiddenTools.includes('superResolution') && (
        <AdjustmentSubSection
          id="superResolution"
          order={toolOrder.indexOf('superResolution')}
          title={t('adjustments.details.superResolution')}
        >
          <SpatialEffect
            enabled={!!adjustments.aiUpscaleEnabled}
            label={t('adjustments.details.aiUpscale')}
            onToggle={(enabled: boolean) =>
              setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, aiUpscaleEnabled: enabled }))
            }
          >
            {aiUpscaleProgress !== undefined ? (
              <div className="flex flex-col items-center justify-center gap-1 p-4 text-text-secondary text-center">
                <div className="flex items-center gap-2">
                  <Loader2 size={16} className="animate-spin shrink-0" />
                  <Text variant={TextVariants.label}>
                    {aiModelDownloadStatus ? t('editor.masks.settings.aiModelDownloading') : t('editor.ai.upscaling')}
                  </Text>
                </div>
                <Text variant={TextVariants.small} className="text-accent">
                  {aiModelDownloadStatus || `${Math.round(aiUpscaleProgress * 100)}%`}
                </Text>
                <button
                  type="button"
                  onClick={() => {
                    invoke(Invokes.CancelAiDenoise).catch((err) => console.warn('Cancel failed:', err));
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, aiUpscaleEnabled: false }));
                  }}
                  className="mt-1 inline-flex items-center gap-1 px-2.5 py-1 rounded text-[11px] bg-surface border border-border-color/60 hover:bg-card-active text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
                >
                  <X size={12} />
                  <span>Cancel</span>
                </button>
              </div>
            ) : (
              <Slider
                resetKey="aiUpscaleDetail"
                label={t('adjustments.details.aiUpscaleDetail')}
                max={100}
                min={0}
                defaultValue={30}
                onChange={(e: SliderChangeEvent) =>
                  handleAdjustmentChange(DetailsAdjustment.AiUpscaleDetail, String(e.target.value))
                }
                step={1}
                value={adjustments.aiUpscaleDetail ?? 30}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            )}
          </SpatialEffect>
        </AdjustmentSubSection>
      )}

      {!isForMask && !hiddenTools.includes('chromaticAberration') && (
        <AdjustmentSubSection
          id="chromaticAberration"
          order={toolOrder.indexOf('chromaticAberration')}
          title={t('adjustments.details.chromaticAberration')}
        >
          <Slider
            resetKey="chromaticAberrationRedCyan"
            label={t('adjustments.details.redCyan')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(DetailsAdjustment.ChromaticAberrationRedCyan, e.target.value)}
            step={1}
            value={adjustments.chromaticAberrationRedCyan}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            resetKey="chromaticAberrationBlueYellow"
            label={t('adjustments.details.blueYellow')}
            max={100}
            min={-100}
            onChange={(e: any) =>
              handleAdjustmentChange(DetailsAdjustment.ChromaticAberrationBlueYellow, e.target.value)
            }
            step={1}
            value={adjustments.chromaticAberrationBlueYellow}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!isForMask && !hiddenTools.includes('negativeConversion') && (
        <AdjustmentSubSection
          id="negativeConversion"
          order={toolOrder.indexOf('negativeConversion')}
          title={t('adjustments.details.negativeConversion')}
        >
          <SpatialEffect
            enabled={!!adjustments.negativeConversionEnabled}
            label={t('adjustments.details.convertNegative')}
            onToggle={(enabled: boolean) =>
              setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, negativeConversionEnabled: enabled }))
            }
          >
            <Slider
              resetKey="negativeRedWeight"
              label={t('adjustments.details.negativeRed')}
              max={2}
              min={0.5}
              defaultValue={1}
              onChange={(e: SliderChangeEvent) =>
                handleFloatAdjustmentChange(DetailsAdjustment.NegativeRedWeight, String(e.target.value))
              }
              step={0.01}
              value={adjustments.negativeRedWeight ?? 1}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <Slider
              resetKey="negativeGreenWeight"
              label={t('adjustments.details.negativeGreen')}
              max={2}
              min={0.5}
              defaultValue={1}
              onChange={(e: SliderChangeEvent) =>
                handleFloatAdjustmentChange(DetailsAdjustment.NegativeGreenWeight, String(e.target.value))
              }
              step={0.01}
              value={adjustments.negativeGreenWeight ?? 1}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <Slider
              resetKey="negativeBlueWeight"
              label={t('adjustments.details.negativeBlue')}
              max={2}
              min={0.5}
              defaultValue={1}
              onChange={(e: SliderChangeEvent) =>
                handleFloatAdjustmentChange(DetailsAdjustment.NegativeBlueWeight, String(e.target.value))
              }
              step={0.01}
              value={adjustments.negativeBlueWeight ?? 1}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <Slider
              resetKey="negativeExposure"
              label={t('adjustments.details.negativeExposure')}
              max={2}
              min={-2}
              onChange={(e: SliderChangeEvent) =>
                handleFloatAdjustmentChange(DetailsAdjustment.NegativeExposure, String(e.target.value))
              }
              step={0.05}
              value={adjustments.negativeExposure ?? 0}
              onDragStateChange={onDragStateChange}
            />
            <Slider
              resetKey="negativeContrast"
              label={t('adjustments.details.negativeContrast')}
              max={2.5}
              min={0.5}
              defaultValue={1}
              onChange={(e: SliderChangeEvent) =>
                handleFloatAdjustmentChange(DetailsAdjustment.NegativeContrast, String(e.target.value))
              }
              step={0.05}
              value={adjustments.negativeContrast ?? 1}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
          </SpatialEffect>
        </AdjustmentSubSection>
      )}
    </div>
  );
}

export default memo(DetailsPanel, areSectionPropsEqual('details'));
