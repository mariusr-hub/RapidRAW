import { useState, useMemo, useCallback } from 'react';
import { ChevronDown, RotateCcw, Sun, Sliders, Sparkles, Flame } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import Slider from '../ui/Slider';
import Text from '../ui/Text';
import { TextVariants, TextColors, TextWeights } from '../../types/typography';
import { Adjustments } from '../../utils/adjustments';

export interface LightsSubPipelineProps {
  adjustments: Adjustments;
  setAdjustments: (adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  isForMask?: boolean;
  appSettings?: any;
  defaultExpanded?: boolean;
}

export default function LightsSubPipeline({
  adjustments,
  setAdjustments,
  onDragStateChange,
  isForMask = false,
  defaultExpanded = true,
}: LightsSubPipelineProps) {
  const { t } = useTranslation();
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  // Normalize float (-1.0 to 1.0 or percentage -100 to 100) to slider percentage (-100 to 100)
  const getPct = useCallback((val: number | undefined): number => {
    if (val === undefined || val === null || isNaN(val)) return 0;
    // If value is in normalized float [-1.5, 1.5], convert to -100..100
    if (Math.abs(val) <= 1.5 && val !== 0) {
      return Math.round(val * 100);
    }
    return Math.round(val);
  }, []);

  const handleSliderChange = useCallback(
    (key: keyof Adjustments, rawValue: number | string) => {
      const num = typeof rawValue === 'string' ? parseFloat(rawValue) : rawValue;
      const normalizedFloat = Number((num / 100).toFixed(3));

      setAdjustments((prev: Adjustments) => {
        const next: any = {
          ...prev,
          [key]: normalizedFloat,
        };
        // Also keep scoped `lights` object synchronized for backend
        next.lights = {
          ...(prev.lights || {}),
          [key]: normalizedFloat,
        };
        return next;
      });
    },
    [setAdjustments],
  );

  const handleReset = useCallback(
    (e?: React.MouseEvent) => {
      e?.stopPropagation();
      setAdjustments((prev: Adjustments) => {
        const next: any = {
          ...prev,
          lightsBrightness: 0.0,
          lightsContrast: 0.0,
          lightsDetails: 0.0,
          lightsTemperature: 0.0,
          lightsTint: 0.0,
          lightsSaturation: 0.0,
          lightsVibrance: 0.0,
        };
        if (next.lights) {
          next.lights = {
            ...next.lights,
            lightsBrightness: 0.0,
            lightsContrast: 0.0,
            lightsDetails: 0.0,
            lightsTemperature: 0.0,
            lightsTint: 0.0,
            lightsSaturation: 0.0,
            lightsVibrance: 0.0,
          };
        }
        return next;
      });
    },
    [setAdjustments],
  );

  const isModified = useMemo(() => {
    return (
      getPct(adjustments.lightsBrightness) !== 0 ||
      getPct(adjustments.lightsContrast) !== 0 ||
      getPct(adjustments.lightsDetails) !== 0 ||
      getPct(adjustments.lightsTemperature) !== 0 ||
      getPct(adjustments.lightsTint) !== 0 ||
      getPct(adjustments.lightsSaturation) !== 0 ||
      getPct(adjustments.lightsVibrance) !== 0
    );
  }, [adjustments, getPct]);

  return (
    <div className="bg-bg-tertiary rounded-lg p-2.5 my-1.5 border border-surface/50 shadow-xs transition-all">
      {/* Collapsible Header */}
      <div
        className="flex items-center justify-between cursor-pointer select-none py-1 group/header"
        onClick={() => setIsExpanded((prev) => !prev)}
      >
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-amber-500/10 text-amber-400">
            <Sun size={15} className="animate-pulse" />
          </div>
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5">
              <Text variant={TextVariants.body} weight={TextWeights.semibold} className="text-text-primary">
                {t('adjustments.lights.title', 'Shadow & Highlight Recovery Details')}
              </Text>
              {isModified && (
                <span className="w-1.5 h-1.5 rounded-full bg-amber-400 ring-2 ring-amber-400/20" />
              )}
            </div>
            <span className="text-[10px] text-text-secondary leading-tight">
              {t('adjustments.lights.desc', 'Dedicated highlight recovery & micro-texture')}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
          {isModified && (
            <button
              onClick={handleReset}
              className="p-1 rounded hover:bg-surface text-text-secondary hover:text-text-primary transition-colors"
              title={t('adjustments.lights.reset', 'Reset Highlights Pipeline')}
            >
              <RotateCcw size={13} />
            </button>
          )}
          <button
            onClick={() => setIsExpanded((prev) => !prev)}
            className="p-1 text-text-secondary group-hover/header:text-text-primary transition-colors"
          >
            <ChevronDown
              size={15}
              className={clsx('transition-transform duration-200', isExpanded ? 'rotate-180' : 'rotate-0')}
            />
          </button>
        </div>
      </div>

      {/* Drawer Content */}
      {isExpanded && (
        <div className="pt-2.5 mt-1 border-t border-surface/40 flex flex-col gap-2">
          {/* Highlight Tonal Controls */}
          <div className="grid grid-cols-1 gap-1">
            <Slider
              label={t('adjustments.lights.brightness', 'Highlights Brightness')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsBrightness)}
              onChange={(e: any) => handleSliderChange('lightsBrightness', e.target.value)}
              onDragStateChange={onDragStateChange}
            />

            <Slider
              label={t('adjustments.lights.contrast', 'Highlights Contrast')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsContrast)}
              onChange={(e: any) => handleSliderChange('lightsContrast', e.target.value)}
              onDragStateChange={onDragStateChange}
            />

            <Slider
              label={t('adjustments.lights.details', 'Highlights Structure')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsDetails)}
              onChange={(e: any) => handleSliderChange('lightsDetails', e.target.value)}
              onDragStateChange={onDragStateChange}
            />
          </div>

          {/* Highlight Color & White Balance Sub-Strip */}
          <div className="bg-surface/30 rounded p-2 border border-surface/30 flex flex-col gap-1 mt-1">
            <div className="flex items-center gap-1.5 mb-1">
              <Sparkles size={12} className="text-amber-400" />
              <span className="text-[11px] font-medium text-text-secondary uppercase tracking-wider">
                Highlight Colorimetry
              </span>
            </div>

            <Slider
              label={t('adjustments.lights.temperature', 'Highlights Warmth')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsTemperature)}
              onChange={(e: any) => handleSliderChange('lightsTemperature', e.target.value)}
              onDragStateChange={onDragStateChange}
            />

            <Slider
              label={t('adjustments.lights.tint', 'Highlights Tint')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsTint)}
              onChange={(e: any) => handleSliderChange('lightsTint', e.target.value)}
              onDragStateChange={onDragStateChange}
            />

            <Slider
              label={t('adjustments.lights.saturation', 'Highlights Saturation')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsSaturation)}
              onChange={(e: any) => handleSliderChange('lightsSaturation', e.target.value)}
              onDragStateChange={onDragStateChange}
            />

            <Slider
              label={t('adjustments.lights.vibrance', 'Highlights Vibrance')}
              min={-100}
              max={100}
              step={1}
              suffix="%"
              defaultValue={0}
              value={getPct(adjustments.lightsVibrance)}
              onChange={(e: any) => handleSliderChange('lightsVibrance', e.target.value)}
              onDragStateChange={onDragStateChange}
            />
          </div>
        </div>
      )}
    </div>
  );
}
