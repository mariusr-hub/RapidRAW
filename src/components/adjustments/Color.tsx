import { useState, useEffect, useMemo } from 'react';
import clsx from 'clsx';
import { Sliders, Target } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import Slider from '../ui/Slider';
import ColorWheel from '../ui/ColorWheel';
import { ColorAdjustment, ColorCalibration, HueSatLum, INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
import { Adjustments, ColorGrading, getAdjustmentToolOrder, getHiddenAdjustmentTools } from '../../utils/adjustments';
import { AppSettings } from '../ui/AppProperties';
import Text from '../ui/Text';
import { TextColors, TextVariants, TextWeights } from '../../types/typography';
import { useShallow } from 'zustand/react/shallow';
import { useEditorStore } from '../../store/useEditorStore';
import AdjustmentSubSection from './AdjustmentSubSection';
import { WhiteBalance } from '../../utils/whiteBalance';

interface ColorProps {
  color: string;
  name: string;
  label: string;
}

interface ColorPanelProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments>): any;
  appSettings: AppSettings | null;
  asShotWhiteBalance?: WhiteBalance;
  isForMask?: boolean;
  isWbPickerActive?: boolean;
  toggleWbPicker?: () => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

interface ColorSwatchProps {
  color: string;
  isActive: boolean;
  name: string;
  ariaLabel: string;
  onClick: (name: string) => void;
}

export const ColorSwatch = ({ color, name, isActive, ariaLabel, onClick }: ColorSwatchProps) => {
  const [isPressed, setIsPressed] = useState(false);
  const [isHovered, setIsHovered] = useState(false);

  const handleMouseDown = () => {
    setIsPressed(true);
  };

  const handleMouseUp = () => {
    setIsPressed(false);
  };

  const handleMouseLeave = () => {
    setIsPressed(false);
    setIsHovered(false);
  };

  const handleMouseEnter = () => {
    setIsHovered(true);
  };

  const handleClick = () => {
    onClick(name);
  };

  const getTransform = () => {
    if (isPressed) return 'scale(0.95)';
    if (isActive) return 'scale(1.1)';
    if (isHovered) return 'scale(1.08)';
    return 'scale(1)';
  };

  return (
    <button
      aria-label={ariaLabel}
      className="relative w-6 h-6 focus:outline-hidden group"
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseLeave}
      onMouseEnter={handleMouseEnter}
      onTouchStart={handleMouseDown}
      onTouchEnd={handleMouseUp}
    >
      <div
        className={`absolute inset-0 rounded-full border-2 transition-all duration-200 ease-out ${
          isActive ? 'border-white opacity-100' : 'scale-100 border-transparent opacity-0'
        }`}
        style={{
          transform: isActive ? (isPressed ? 'scale(1.1)' : 'scale(1.25)') : undefined,
          transition: isPressed
            ? 'transform 100ms cubic-bezier(0.4, 0, 0.2, 1), opacity 200ms ease-out'
            : 'transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1), opacity 200ms ease-out',
        }}
      />

      <div
        className={`absolute inset-0 rounded-full transition-all duration-150 ease-out ${
          isActive ? 'shadow-lg' : 'shadow-md'
        }`}
        style={{
          backgroundColor: color,
          transform: getTransform(),
          transition: isPressed
            ? 'transform 100ms cubic-bezier(0.4, 0, 0.2, 1)'
            : 'transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1)',
        }}
      />
    </button>
  );
};

const ColorGradingPanel = ({ adjustments, setAdjustments, onDragStateChange }: ColorPanelProps) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<'3way' | 'global'>('3way');
  const [isExpanded, setIsExpanded] = useState(false);
  const colorGrading = adjustments.colorGrading || INITIAL_ADJUSTMENTS.colorGrading;

  const handleChange = (grading: ColorGrading, newValue: HueSatLum) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorGrading: {
        ...(prev.colorGrading || INITIAL_ADJUSTMENTS.colorGrading),
        [grading]: newValue,
      },
    }));
  };

  const handleColorGradingSliderChange = (grading: ColorGrading, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorGrading: {
        ...(prev.colorGrading || INITIAL_ADJUSTMENTS.colorGrading),
        [grading]: parseFloat(value),
      },
    }));
  };

  const tabs = useMemo(
    () => [
      {
        id: '3way',
        icon: (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="12" cy="6" r="4.5" />
            <circle cx="5" cy="18" r="4.5" />
            <circle cx="19" cy="18" r="4.5" />
          </svg>
        ),
      },
      {
        id: 'global',
        icon: (
          <div className="w-3.5 h-3.5 rounded-full" style={{ background: 'linear-gradient(to top, #666, #fff)' }} />
        ),
      },
    ],
    [],
  );

  return (
    <div>
      <div className="flex items-center justify-start gap-2 mb-4 mt-2 px-1">
        {tabs.map((tab) => {
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as '3way' | 'global')}
              className={`w-7 h-7 rounded-full flex items-center justify-center transition-all focus:outline-none
                ${
                  isActive
                    ? 'ring-2 ring-offset-2 ring-offset-surface ring-accent text-text-primary'
                    : 'bg-bg-secondary text-text-secondary hover:text-text-primary hover:bg-bg-secondary/80'
                }`}
            >
              {tab.icon}
            </button>
          );
        })}

        <div className="w-px h-5 bg-text-secondary/20 mx-1" />

        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className={`w-7 h-7 rounded-full flex items-center justify-center transition-all focus:outline-none
            ${
              isExpanded
                ? 'bg-accent text-button-text'
                : 'bg-bg-secondary text-text-secondary hover:text-text-primary hover:bg-bg-secondary/80'
            }`}
          data-tooltip={t('adjustments.color.toggleSliders')}
        >
          <Sliders size={14} />
        </button>
      </div>

      <div className="relative w-full mb-4">
        <AnimatePresence mode="wait">
          {activeTab === '3way' ? (
            <motion.div
              key="3way"
              initial={{ opacity: 0, x: -15 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -15 }}
              transition={{ duration: 0.2 }}
              className="w-full"
            >
              <div className="flex justify-center mb-4">
                <div className="w-[calc(50%-0.5rem)]">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.midtones}
                    label={t('adjustments.color.grading.midtones')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Midtones, val)}
                    value={colorGrading.midtones}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
              </div>
              <div className="flex justify-between mb-2 gap-4">
                <div className="w-full flex-1 min-w-0">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.shadows}
                    label={t('adjustments.color.grading.shadows')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Shadows, val)}
                    value={colorGrading.shadows}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
                <div className="w-full flex-1 min-w-0">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.highlights}
                    label={t('adjustments.color.grading.highlights')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Highlights, val)}
                    value={colorGrading.highlights}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="global"
              initial={{ opacity: 0, x: 15 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 15 }}
              transition={{ duration: 0.2 }}
              className="w-full flex justify-center pb-2"
            >
              <div className="w-full max-w-70">
                <ColorWheel
                  defaultValue={INITIAL_ADJUSTMENTS.colorGrading.global}
                  label={t('adjustments.color.grading.global')}
                  onChange={(val: HueSatLum) => handleChange(ColorGrading.Global, val)}
                  value={colorGrading.global || INITIAL_ADJUSTMENTS.colorGrading.global}
                  onDragStateChange={onDragStateChange}
                  isExpanded={isExpanded}
                />
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div>
        <Slider
          defaultValue={50}
          label={t('adjustments.color.grading.blending')}
          max={100}
          min={0}
          onChange={(e: any) => handleColorGradingSliderChange(ColorGrading.Blending, e.target.value)}
          step={1}
          value={colorGrading.blending}
          onDragStateChange={onDragStateChange}
        />
        <Slider
          defaultValue={0}
          label={t('adjustments.color.grading.balance')}
          max={100}
          min={-100}
          onChange={(e: any) => handleColorGradingSliderChange(ColorGrading.Balance, e.target.value)}
          step={1}
          value={colorGrading.balance}
          onDragStateChange={onDragStateChange}
        />
      </div>
    </div>
  );
};

const ColorCalibrationPanel = ({ adjustments, setAdjustments, onDragStateChange }: ColorPanelProps) => {
  const { t } = useTranslation();
  const [activePrimary, setActivePrimary] = useState('red');
  const colorCalibration = adjustments.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration;

  const PRIMARY_COLORS = useMemo(
    () => [
      { name: 'red', color: '#f87171', label: t('adjustments.color.calibration.colors.red') },
      { name: 'green', color: '#4ade80', label: t('adjustments.color.calibration.colors.green') },
      { name: 'blue', color: '#60a5fa', label: t('adjustments.color.calibration.colors.blue') },
    ],
    [t],
  );

  const handleShadowsChange = (value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorCalibration: {
        ...(prev.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration),
        shadowsTint: parseFloat(value),
      },
    }));
  };

  const handlePrimaryChange = (key: 'Hue' | 'Saturation', value: string) => {
    const fullKey = `${activePrimary}${key}` as keyof ColorCalibration;
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorCalibration: {
        ...(prev.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration),
        [fullKey]: parseFloat(value),
      },
    }));
  };

  const currentValues = {
    hue: colorCalibration[`${activePrimary}Hue` as keyof ColorCalibration] || 0,
    saturation: colorCalibration[`${activePrimary}Saturation` as keyof ColorCalibration] || 0,
  };

  const trackSuffix = `${activePrimary}s`;

  return (
    <div>
      <div>
        <Text color={TextColors.primary} weight={TextWeights.medium} className="mb-1">
          {t('adjustments.color.calibration.shadows')}
        </Text>
        <Slider
          label={t('adjustments.color.calibration.tint')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={colorCalibration.shadowsTint}
          onChange={(e: any) => handleShadowsChange(e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName="tint-gradient-track"
        />
      </div>
      <div className="mt-3">
        <Text color={TextColors.primary} weight={TextWeights.medium} className="mb-3">
          {t('adjustments.color.calibration.primaries')}
        </Text>
        <div className="flex justify-center gap-6 mb-4 px-1">
          {PRIMARY_COLORS.map(({ name, color, label }) => (
            <ColorSwatch
              color={color}
              isActive={activePrimary === name}
              key={name}
              name={name}
              onClick={setActivePrimary}
              ariaLabel={t('adjustments.color.ariaSelectColor', { name: label })}
            />
          ))}
        </div>
        <Slider
          label={t('adjustments.color.calibration.hue')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={currentValues.hue}
          onChange={(e: any) => handlePrimaryChange('Hue', e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName={`hue-slider-${trackSuffix}`}
        />
        <Slider
          label={t('adjustments.color.calibration.saturation')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={currentValues.saturation}
          onChange={(e: any) => handlePrimaryChange('Saturation', e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName={`sat-slider-${trackSuffix}`}
        />
      </div>
    </div>
  );
};

export default function ColorPanel({
  adjustments,
  setAdjustments,
  appSettings,
  isForMask = false,
  isWbPickerActive = false,
  toggleWbPicker,
  onDragStateChange,
  asShotWhiteBalance,
}: ColorPanelProps) {
  const { t } = useTranslation();
  const [activeColor, setActiveColor] = useState('reds');
  const hiddenTools = getHiddenAdjustmentTools(appSettings?.adjustmentLayout);
  const toolOrder = getAdjustmentToolOrder('color', appSettings?.adjustmentLayout?.toolOrder);

  const { isTatActive, tatMode, setEditor } = useEditorStore(
    useShallow((state) => ({
      isTatActive: state.isTatActive,
      tatMode: state.tatMode,
      setEditor: state.setEditor,
    })),
  );

  const HSL_COLORS = useMemo<Array<ColorProps>>(
    () => [
      { name: 'reds', color: '#f87171', label: t('adjustments.color.mixerColors.reds') },
      { name: 'oranges', color: '#fb923c', label: t('adjustments.color.mixerColors.oranges') },
      { name: 'yellows', color: '#facc15', label: t('adjustments.color.mixerColors.yellows') },
      { name: 'greens', color: '#4ade80', label: t('adjustments.color.mixerColors.greens') },
      { name: 'aquas', color: '#2dd4bf', label: t('adjustments.color.mixerColors.aquas') },
      { name: 'blues', color: '#60a5fa', label: t('adjustments.color.mixerColors.blues') },
      { name: 'purples', color: '#a78bfa', label: t('adjustments.color.mixerColors.purples') },
      { name: 'magentas', color: '#f472b6', label: t('adjustments.color.mixerColors.magentas') },
    ],
    [t],
  );

  const colorHueMap = useMemo<Record<string, number>>(
    () => ({
      reds: 0,
      oranges: 30,
      yellows: 60,
      greens: 120,
      aquas: 180,
      blues: 240,
      purples: 300,
      magentas: 340,
    }),
    [],
  );

  const currentHsl = adjustments?.hsl?.[activeColor] || { hue: 0, saturation: 0, luminance: 0 };
  const baseHue = colorHueMap[activeColor] || 0;
  const effectiveHue = baseHue + (currentHsl.hue || 0);

  useEffect(() => {
    const normalizedHue = ((effectiveHue % 360) + 360) % 360;
    const effectiveSaturation = (currentHsl.saturation + 100) / 2;

    document.documentElement.style.setProperty(`--hsl-mixer-hue-${activeColor}`, normalizedHue.toString());
    document.documentElement.style.setProperty(`--hsl-mixer-sat-${activeColor}`, `${effectiveSaturation}%`);
  }, [effectiveHue, currentHsl.saturation, activeColor]);

  const handleAdjustmentChange = (key: ColorAdjustment, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: parseFloat(value) }));
  };


  const handleHslChange = (key: ColorAdjustment, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      hsl: {
        ...(prev.hsl || {}),
        [activeColor]: {
          ...(prev.hsl?.[activeColor] || {}),
          [key]: parseFloat(value),
        },
      },
    }));
  };

  const [splitWarmth, setSplitWarmth] = useState<number>(0);
  const [colorHarmonizer, setColorHarmonizer] = useState<number>(0);
  const [selectedFidelity, setSelectedFidelity] = useState<string | null>(null);

  const handleSplitWarmthChange = (val: number) => {
    setSplitWarmth(val);
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorGrading: {
        ...(prev.colorGrading || ({} as any)),
        highlights: {
          ...(prev.colorGrading?.highlights || { luminance: 0 }),
          hue: val >= 0 ? 40 : 218,
          saturation: Math.min(100, Math.round(Math.abs(val) * 0.35)),
        },
        shadows: {
          ...(prev.colorGrading?.shadows || { luminance: 0 }),
          hue: val >= 0 ? 218 : 40,
          saturation: Math.min(100, Math.round(Math.abs(val) * 0.25)),
        },
      },
    }));
  };

  const handleHarmonizerChange = (val: number) => {
    setColorHarmonizer(val);
    const factor = val / 100;
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorCalibration: {
        ...(prev.colorCalibration || {
          shadowsTint: 0,
          redHue: 0,
          redSaturation: 0,
          greenHue: 0,
          greenSaturation: 0,
          blueHue: 0,
          blueSaturation: 0,
        }),
        redHue: Math.round(15 * factor),
        redSaturation: Math.round(12 * factor),
        blueHue: Math.round(-10 * factor),
        blueSaturation: Math.round(18 * factor),
        shadowsTint: Math.round(-5 * factor),
      },
    }));
  };

  const handleFidelitySelect = (mode: 'standard' | 'vivid' | 'golden' | 'cyan') => {
    setSelectedFidelity(mode);
    setAdjustments((prev: Partial<Adjustments>) => {
      switch (mode) {
        case 'standard':
          return {
            ...prev,
            vibrance: 28,
            saturation: 6,
            highlights: Math.min(prev.highlights ?? 0, -5),
          };
        case 'vivid':
          return {
            ...prev,
            vibrance: 48,
            saturation: 14,
            colorCalibration: {
              ...(prev.colorCalibration || {
                shadowsTint: 0,
                redHue: 0,
                redSaturation: 0,
                greenHue: 0,
                greenSaturation: 0,
                blueHue: 0,
                blueSaturation: 0,
              }),
              blueSaturation: 12,
              redSaturation: 6,
            },
          };
        case 'golden':
          return {
            ...prev,
            vibrance: 30,
            temperature: (prev.temperature ?? 0) + 12,
            tint: (prev.tint ?? 0) + 4,
            colorCalibration: {
              ...(prev.colorCalibration || {
                shadowsTint: 0,
                redHue: 0,
                redSaturation: 0,
                greenHue: 0,
                greenSaturation: 0,
                blueHue: 0,
                blueSaturation: 0,
              }),
              redSaturation: 10,
            },
          };
        case 'cyan':
          return {
            ...prev,
            vibrance: 25,
            temperature: (prev.temperature ?? 0) - 14,
            tint: (prev.tint ?? 0) - 6,
            colorCalibration: {
              ...(prev.colorCalibration || {
                shadowsTint: 0,
                redHue: 0,
                redSaturation: 0,
                greenHue: 0,
                greenSaturation: 0,
                blueHue: 0,
                blueSaturation: 0,
              }),
              blueSaturation: 15,
            },
          };
      }
    });
  };

  const hue_slider = `hue-slider-${activeColor}`;
  const saturation_slider = `sat-slider-${activeColor}`;
  const luminance_slider = `lum-slider-${activeColor}`;

  return (
    <div className="flex flex-col gap-4">
      {/* Color Fidelity & Cast Correction Sub-Card */}
      <div className="p-2.5 rounded-md bg-card-active border border-border-color/50 flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-amber-300">
            Color Fidelity & Cast Correction
          </span>
          <span className="text-[10px] text-text-secondary">Subtractive Film Density</span>
        </div>

        {/* 4-Mode Color Fidelity Buttons */}
        <div className="grid grid-cols-4 gap-1 text-[10px]">
          {[
            { id: 'standard', label: 'Standard' },
            { id: 'vivid', label: 'Vivid' },
            { id: 'golden', label: 'Golden' },
            { id: 'cyan', label: 'Cool Cyan' },
          ].map((mode) => (
            <button
              key={mode.id}
              type="button"
              onClick={() => handleFidelitySelect(mode.id as any)}
              className={clsx(
                'py-1 px-1 rounded border text-center transition-colors truncate',
                selectedFidelity === mode.id
                  ? 'bg-amber-500/20 border-amber-500/60 text-amber-300 font-medium'
                  : 'bg-surface hover:bg-surface-secondary border-border-color/40 text-text-secondary hover:text-text-primary',
              )}
            >
              {mode.label}
            </button>
          ))}
        </div>

        {/* Split Warmth Slider */}
        <Slider
          label="Split Warmth (Cool Shadows / Warm HL)"
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={splitWarmth}
          onChange={(e: any) => handleSplitWarmthChange(parseInt(e.target.value, 10))}
          onDragStateChange={onDragStateChange}
        />

        {/* Complementary Color Harmonizer */}
        <Slider
          label="Complementary Harmonizer (L1 / L2)"
          min={0}
          max={100}
          step={1}
          defaultValue={0}
          value={colorHarmonizer}
          onChange={(e: any) => handleHarmonizerChange(parseInt(e.target.value, 10))}
          onDragStateChange={onDragStateChange}
        />
      </div>


      {!hiddenTools.includes('colorPresence') && (
        <AdjustmentSubSection
          id="colorPresence"
          order={toolOrder.indexOf('colorPresence')}
          title={t('adjustments.color.presence')}
        >
          <Slider
            label={t('adjustments.color.vibrance')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Vibrance, e.target.value)}
            step={1}
            value={adjustments.vibrance || 0}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            label={t('adjustments.color.saturation')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Saturation, e.target.value)}
            step={1}
            value={adjustments.saturation || 0}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('hue') && (
        <AdjustmentSubSection
          id="hue"
          order={toolOrder.indexOf('hue')}
          title={isForMask ? t('adjustments.color.localHue') : t('adjustments.color.hue')}
        >
          <Slider
            label={t('adjustments.color.hue')}
            max={180}
            min={-180}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Hue, e.target.value)}
            step={1}
            value={adjustments.hue || 0}
            trackClassName="hue-range-track"
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('colorGrading') && (
        <AdjustmentSubSection
          id="colorGrading"
          order={toolOrder.indexOf('colorGrading')}
          title={t('adjustments.color.colorGrading')}
        >
          <ColorGradingPanel
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            appSettings={appSettings}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('colorMixer') && (
        <AdjustmentSubSection
          id="colorMixer"
          order={toolOrder.indexOf('colorMixer')}
          title={t('adjustments.color.colorMixer')}
          actions={
            !isForMask && (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  const isCurrentlyActive = isTatActive && tatMode === 'hsl_sat';
                  setEditor({
                    isTatActive: !isCurrentlyActive,
                    tatMode: !isCurrentlyActive ? 'hsl_sat' : null,
                  });
                }}
                className={`px-2 py-0.5 rounded text-[11px] font-semibold flex items-center gap-1 transition-all border cursor-pointer ${
                  isTatActive && tatMode?.startsWith('hsl')
                    ? 'bg-accent text-white border-accent shadow-sm'
                    : 'bg-surface/60 text-text-secondary border-surface hover:text-white'
                }`}
                title="Targeted Adjustment Tool (Click on image and drag to adjust color saturation/luminance/hue directly on canvas)"
              >
                <Target size={12} className={isTatActive && tatMode?.startsWith('hsl') ? 'animate-pulse' : ''} />
                <span>{isTatActive && tatMode?.startsWith('hsl') ? 'Sampler On' : 'Tone Sampler'}</span>
              </button>
            )
          }
        >
          <div className="flex justify-between mb-4 px-1">
            {HSL_COLORS.map(({ name, color, label }) => (
              <ColorSwatch
                color={color}
                isActive={activeColor === name}
                key={name}
                name={name}
                onClick={setActiveColor}
                ariaLabel={t('adjustments.color.ariaSelectColor', { name: label })}
              />
            ))}
          </div>
          <Slider
            label={t('adjustments.color.hue')}
            max={100}
            min={-100}
            onChange={(e: any) => handleHslChange(ColorAdjustment.Hue, e.target.value)}
            step={1}
            value={currentHsl.hue}
            trackClassName={hue_slider}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            label={t('adjustments.color.saturation')}
            max={100}
            min={-100}
            onChange={(e: any) => handleHslChange(ColorAdjustment.Saturation, e.target.value)}
            step={1}
            value={currentHsl.saturation}
            trackClassName={saturation_slider}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            label={t('adjustments.color.luminance')}
            max={100}
            min={-100}
            onChange={(e: any) => handleHslChange(ColorAdjustment.Luminance, e.target.value)}
            step={1}
            value={currentHsl.luminance}
            trackClassName={luminance_slider}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!isForMask && !hiddenTools.includes('colorCalibration') && (
        <AdjustmentSubSection
          id="colorCalibration"
          order={toolOrder.indexOf('colorCalibration')}
          title={t('adjustments.color.calibration.title')}
        >
          <ColorCalibrationPanel
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            appSettings={appSettings}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}
    </div>
  );
}
