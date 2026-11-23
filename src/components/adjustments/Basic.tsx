import { motion } from 'framer-motion';
import clsx from 'clsx';
import { invoke } from '@tauri-apps/api/core';
import { Check, ChevronDown, Flower2, Heart, Loader2, Sparkles, Sun, Zap } from 'lucide-react';
import { toast } from 'react-toastify';
import Slider from '../ui/Slider';
import Button from '../ui/Button';
import { Adjustments, BasicAdjustment } from '../../utils/adjustments';
import { useEffect, useRef, useState, useMemo, memo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { areSectionPropsEqual } from '../../utils/adjustments';
import { Invokes } from '../../types/invokes';
import { useEditorStore } from '../../store/useEditorStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import ReferencePresetsRibbon from './ReferencePresetsRibbon';
import LightsSubPipeline from './LightsSubPipeline';
import SupercontrastCurve from './SupercontrastCurve';

function scaleSmartParam(nominal: number, minVal: number, maxCeiling: number, pct: number): number {
  const s = Math.max(0, Math.min(200, pct));
  const delta = (s - 100) / 100;
  return delta <= 0 ? nominal + (nominal - minVal) * delta : nominal + (maxCeiling - nominal) * delta;
}

interface SmartDiagnostics {
  sceneName: string;
  tintCastStatus: string;
  perPixelEvStatus: string;
  noiseGateStatus: string;
}

type WeddingScene = 'master' | 'bridalGlow' | 'ceremony' | 'goldenHour' | 'reception';
type MacroScene = 'master' | 'droplets' | 'insects' | 'botanical' | 'jewelry';
type AstroScene = 'master' | 'milkyWay' | 'starryLandscape' | 'deepSky' | 'moonlitAurora';

interface BasicAdjustmentsProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)): any;
  isForMask?: boolean;
  onDragStateChange?: (isDragging: boolean) => void;
  appSettings?: any;
}

interface ToneMapperSwitchProps {
  selectedMapper: string;
  onMapperChange: (mapper: string) => void;
  exposureValue: number;
  onExposureChange: (value: number) => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

const ToneMapperSwitch = ({
  selectedMapper,
  onMapperChange,
  exposureValue,
  onExposureChange,
  onDragStateChange,
}: ToneMapperSwitchProps) => {
  const { t } = useTranslation();
  const [bubbleStyle, setBubbleStyle] = useState({});
  const isInitialAnimation = useRef(true);
  const [isLabelHovered, setIsLabelHovered] = useState(false);

  const toneMapperOptions = useMemo(
    () => [
      {
        id: 'basic',
        label: t('adjustments.basic.mappers.basic'),
        title: t('adjustments.basic.mappers.basicDesc'),
      },
      {
        id: 'agx',
        label: t('adjustments.basic.mappers.agx'),
        title: t('adjustments.basic.mappers.agxDesc'),
      },
      {
        id: 'legacy',
        label: t('adjustments.basic.mappers.legacy'),
        title: t('adjustments.basic.mappers.legacyDesc'),
      },
    ],
    [t],
  );

  const handleReset = () => {
    onMapperChange('basic');
    onExposureChange(0);
  };

  useEffect(() => {
    const selectedIndex = toneMapperOptions.findIndex((m) => m.id === selectedMapper);
    const safeIndex = selectedIndex >= 0 ? selectedIndex : 0;

    const widthPercent = 100 / toneMapperOptions.length;
    const targetX = `${safeIndex * 100}%`;
    const targetWidth = `${widthPercent}%`;

    if (isInitialAnimation.current) {
      let initialX;
      if (selectedMapper === 'agx') {
        initialX = `${toneMapperOptions.length * 100}%`;
      } else {
        initialX = '-25%';
      }

      setBubbleStyle({
        x: [initialX, targetX],
        width: targetWidth,
      });
      isInitialAnimation.current = false;
    } else {
      setBubbleStyle({
        x: targetX,
        width: targetWidth,
      });
    }
  }, [selectedMapper, toneMapperOptions]);

  return (
    <div className="group mb-3">
      <div className="flex justify-between items-center mb-2">
        <div
          className="grid cursor-pointer"
          onClick={handleReset}
          onDoubleClick={handleReset}
          onMouseEnter={() => setIsLabelHovered(true)}
          onMouseLeave={() => setIsLabelHovered(false)}
        >
          <span
            aria-hidden={isLabelHovered}
            className={`col-start-1 row-start-1 text-sm font-medium text-text-secondary select-none transition-opacity duration-200 ease-in-out ${
              isLabelHovered ? 'opacity-0' : 'opacity-100'
            }`}
          >
            {t('adjustments.basic.toneMapper')}
          </span>
          <span
            aria-hidden={!isLabelHovered}
            className={`col-start-1 row-start-1 text-sm font-medium text-text-primary select-none transition-opacity duration-200 ease-in-out pointer-events-none ${
              isLabelHovered ? 'opacity-100' : 'opacity-0'
            }`}
          >
            {t('adjustments.basic.reset')}
          </span>
        </div>
      </div>
      <div className="w-full p-2 pb-1 bg-card-active rounded-md">
        <div className="relative flex w-full">
          <motion.div
            className="absolute top-0 bottom-0 z-0 bg-accent"
            style={{ borderRadius: 6 }}
            animate={bubbleStyle}
            transition={{ type: 'spring', bounce: 0.2, duration: 0.6 }}
          />
          {toneMapperOptions.map((mapper) => (
            <button
              key={mapper.id}
              data-tooltip={mapper.title}
              onClick={() => onMapperChange(mapper.id)}
              className={clsx(
                'relative flex-1 flex items-center justify-center gap-2 px-3 p-1.5 text-sm font-medium rounded-md transition-colors',
                {
                  'text-text-primary hover:bg-surface': selectedMapper !== mapper.id,
                  'text-button-text': selectedMapper === mapper.id,
                },
              )}
              style={{ WebkitTapHighlightColor: 'transparent' }}
            >
              <span className="relative z-10 flex items-center">{mapper.label}</span>
            </button>
          ))}
        </div>
        <div className="mt-2.5 px-1">
          <Slider
            label={t('adjustments.basic.exposure')}
            max={5}
            min={-5}
            onChange={(e: any) => onExposureChange(parseFloat(e.target.value))}
            speedEditId={BasicAdjustment.Exposure}
            step={0.01}
            resetKey="exposure"
            value={exposureValue}
            trackClassName="bg-surface"
            onDragStateChange={onDragStateChange}
          />
        </div>
      </div>
    </div>
  );
};

function BasicAdjustments({
  adjustments,
  setAdjustments,
  isForMask = false,
  onDragStateChange,
  appSettings,
}: BasicAdjustmentsProps) {
  const { t } = useTranslation();
  const [isAutoTuning, setIsAutoTuning] = useState(false);
  const [isSupercontrastOpen, setIsSupercontrastOpen] = useState(false);

  const [isWeddingMenuOpen, setIsWeddingMenuOpen] = useState(false);
  const [selectedWeddingScene, setSelectedWeddingScene] = useState<WeddingScene>('master');
  const weddingMenuRef = useRef<HTMLDivElement>(null);

  const [isMacroMenuOpen, setIsMacroMenuOpen] = useState(false);
  const [selectedMacroScene, setSelectedMacroScene] = useState<MacroScene>('master');
  const macroMenuRef = useRef<HTMLDivElement>(null);

  const [isAstroMenuOpen, setIsAstroMenuOpen] = useState(false);
  const [selectedAstroScene, setSelectedAstroScene] = useState<AstroScene>('master');
  const astroMenuRef = useRef<HTMLDivElement>(null);

  const [selectedSkyPreset, setSelectedSkyPreset] = useState<string>('polar');
  const [isSkyMenuOpen, setIsSkyMenuOpen] = useState<boolean>(false);
  const skyMenuRef = useRef<HTMLDivElement>(null);

  const [isAutoAnalyzing, setIsAutoAnalyzing] = useState(false);
  const [detectedSceneBadge, setDetectedSceneBadge] = useState<string | null>(null);
  const [masterStrength, setMasterStrength] = useState<number>(100);
  const [enhanceMacro, setEnhanceMacro] = useState<number>(100);
  const [colorMacro, setColorMacro] = useState<number>(100);
  const [skinMacro, setSkinMacro] = useState<number>(100);
  const [smartDiag, setSmartDiag] = useState<SmartDiagnostics | null>(null);
  const smartBaselineRef = useRef<Partial<Adjustments> | null>(null);
  const lastAutoAnalyzedPathRef = useRef<string | null>(null);

  const selectedImage = useEditorStore((state) => state.selectedImage);
  const handleSettingsChange = useSettingsStore((state) => state.handleSettingsChange);
  const autoAnalyzeOnOpen = appSettings?.autoAnalyzeOnOpen === true;

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (weddingMenuRef.current && !weddingMenuRef.current.contains(event.target as Node)) {
        setIsWeddingMenuOpen(false);
      }
      if (macroMenuRef.current && !macroMenuRef.current.contains(event.target as Node)) {
        setIsMacroMenuOpen(false);
      }
      if (astroMenuRef.current && !astroMenuRef.current.contains(event.target as Node)) {
        setIsAstroMenuOpen(false);
      }
      if (skyMenuRef.current && !skyMenuRef.current.contains(event.target as Node)) {
        setIsSkyMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  useEffect(() => {
    smartBaselineRef.current = null;
    setSmartDiag(null);
    setDetectedSceneBadge(null);
    setMasterStrength(100);
    setEnhanceMacro(100);
    setColorMacro(100);
    setSkinMacro(100);
  }, [selectedImage?.path]);

  const hasSupercontrastActive =
    Math.abs(adjustments.supercontrastShadows ?? 0) > 1e-4 ||
    Math.abs(adjustments.supercontrastMidtones ?? 0) > 1e-4 ||
    Math.abs(adjustments.supercontrastHighlights ?? 0) > 1e-4 ||
    Math.abs(adjustments.supercontrastPivots ?? 0) > 1e-4;

  const handleSkySculpt = async (preset: string = selectedSkyPreset) => {
    setSelectedSkyPreset(preset);
    setIsSkyMenuOpen(false);
    try {
      const res: any = await invoke(Invokes.ApplyAiSkySculpt, { preset, intensity: 100 });
      if (res && res.mask_adjustments) {
        const current = useEditorStore.getState().adjustments;
        const next = { ...current, ...res.mask_adjustments };
        setAdjustments(res.mask_adjustments);
        useEditorStore.getState().pushHistory(next, 'Sky Sculpt');
        setDetectedSceneBadge(`Sky Sculpted: ${res.preset_name}`);
      }
    } catch (err) {
      console.error('Sky sculpt error:', err);
    }
  };

  const handleAutoEnhance = useCallback(async () => {
    onDragStateChange?.(true);
    try {
      setIsAutoAnalyzing(true);
      const res: any = await invoke(Invokes.AnalyzeAndPolishActiveImage, {
        intensity: 100,
        autoStraighten: true,
        toneStyle: 'filmic',
        skinProtection: true,
      });
      if (res && res.adjustments) {
        setDetectedSceneBadge(res.sceneName || null);
        const adj = res.adjustments;
        smartBaselineRef.current = adj;
        setMasterStrength(100);
        setEnhanceMacro(100);
        setColorMacro(100);
        setSkinMacro(100);
        setSmartDiag({
          sceneName: res.sceneName || 'Universal',
          tintCastStatus:
            Math.abs(adj.tint || 0) > 2 || Math.abs(adj.temperature || 0) > 2 ? 'Tint Cast Removed' : 'Clean WB',
          perPixelEvStatus: `${(adj.exposure || 0) >= 0 ? '+' : ''}${Number(adj.exposure || 0).toFixed(2)} EV`,
          noiseGateStatus:
            (adj.lumaNoiseReduction || 0) > 0 ? `Denoise (${adj.lumaNoiseReduction}%)` : 'No Noise (Bypassed)',
        });
        const current = useEditorStore.getState().adjustments;
        const next = { ...current, ...adj };
        setAdjustments((prev: Partial<Adjustments>) => ({
          ...prev,
          ...adj,
        }));
        useEditorStore.getState().setEditor({
          pendingAutoEdit: {
            path: selectedImage?.path ?? '',
            scene: res.sceneName || 'Smart Tone',
            previousAdjustments: current,
          },
        });
        useEditorStore.getState().pushHistory(next, 'Smart Tone');
      }
    } catch (err) {
      console.error('Semantic AI polish error:', err);
      try {
        const current = useEditorStore.getState().adjustments;
        const results: any = await invoke(Invokes.CalculateAutoAdjustments, { jsAdjustments: current });
        if (results) {
          const patch: Partial<Adjustments> = {};
          if (typeof results.exposure === 'number') patch.exposure = results.exposure;
          if (typeof results.contrast === 'number') patch.contrast = results.contrast;
          if (typeof results.highlights === 'number') patch.highlights = results.highlights;
          if (typeof results.shadows === 'number') patch.shadows = results.shadows;
          if (typeof results.whites === 'number') patch.whites = results.whites;
          if (typeof results.blacks === 'number') patch.blacks = results.blacks;
          if (typeof results.clarity === 'number') patch.clarity = results.clarity;
          if (typeof results.dehaze === 'number') patch.dehaze = results.dehaze;
          if (typeof results.vibrance === 'number') patch.vibrance = results.vibrance;
          if (typeof results.vignetteAmount === 'number') patch.vignetteAmount = results.vignetteAmount;
          setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, ...patch }));
          useEditorStore.getState().setEditor({
            pendingAutoEdit: {
              path: selectedImage?.path ?? '',
              scene: 'Auto Enhance',
              previousAdjustments: current,
            },
          });
          useEditorStore.getState().pushHistory({ ...current, ...patch }, 'Auto Enhance');
        }
      } catch (e) {
        console.error('Fallback auto enhance error:', e);
      }
    } finally {
      setIsAutoAnalyzing(false);
      onDragStateChange?.(false);
    }
  }, [onDragStateChange, setAdjustments]);

  const handleAutoEnhanceRef = useRef(handleAutoEnhance);
  handleAutoEnhanceRef.current = handleAutoEnhance;

  useEffect(() => {
    const onSmartDiag = (evt: Event) => {
      const detail = (evt as CustomEvent).detail;
      if (!detail?.adjustments) return;
      const adj = detail.adjustments;
      smartBaselineRef.current = adj;
      setDetectedSceneBadge(detail.sceneName || null);
      setMasterStrength(100);
      setEnhanceMacro(100);
      setColorMacro(100);
      setSkinMacro(100);
      setSmartDiag({
        sceneName: detail.sceneName || 'Universal',
        tintCastStatus:
          Math.abs(adj.tint || 0) > 2 || Math.abs(adj.temperature || 0) > 2 ? 'Tint Cast Removed' : 'Clean WB',
        perPixelEvStatus: `${(adj.exposure || 0) >= 0 ? '+' : ''}${Number(adj.exposure || 0).toFixed(2)} EV`,
        noiseGateStatus:
          (adj.lumaNoiseReduction || 0) > 0 ? `Denoise (${adj.lumaNoiseReduction}%)` : 'No Noise (Bypassed)',
      });
    };
    const onQuickSmartTone = () => {
      handleAutoEnhanceRef.current();
    };
    window.addEventListener('rapidraw-smart-diag', onSmartDiag);
    window.addEventListener('rapidraw-quick-smart-tone', onQuickSmartTone);
    return () => {
      window.removeEventListener('rapidraw-smart-diag', onSmartDiag);
      window.removeEventListener('rapidraw-quick-smart-tone', onQuickSmartTone);
    };
  }, []);

  useEffect(() => {
    if (!isForMask && autoAnalyzeOnOpen && selectedImage?.isReady && selectedImage.path) {
      if (lastAutoAnalyzedPathRef.current !== selectedImage.path) {
        lastAutoAnalyzedPathRef.current = selectedImage.path;
        handleAutoEnhanceRef.current();
      }
    }
  }, [isForMask, autoAnalyzeOnOpen, selectedImage?.isReady, selectedImage?.path]);

  const applyCoupledSmartSliders = (nextMaster: number, nextEnhance: number, nextColor: number, nextSkin: number) => {
    if (!smartBaselineRef.current) {
      smartBaselineRef.current = { ...useEditorStore.getState().adjustments };
    }
    const base = smartBaselineRef.current;
    if (!base) return;
    const toneScale = (nextMaster * nextEnhance) / 100;
    const colScale = (nextMaster * nextColor) / 100;
    const sknScale = (nextMaster * nextSkin) / 100;

    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      exposure: Number(scaleSmartParam(base.exposure ?? 0, 0, 1.25, toneScale).toFixed(2)),
      contrast: Math.round(scaleSmartParam(base.contrast ?? 0, 0, 45, toneScale)),
      highlights: Math.round(scaleSmartParam(base.highlights ?? 0, 0, -75, toneScale)),
      shadows: Math.round(scaleSmartParam(base.shadows ?? 0, 0, 75, toneScale)),
      whites: Math.round(scaleSmartParam(base.whites ?? 0, 0, 45, toneScale)),
      blacks: Math.round(scaleSmartParam(base.blacks ?? 0, 0, -35, toneScale)),
      clarity: Math.round(scaleSmartParam(base.clarity ?? 0, 0, 35, toneScale)),
      dehaze: Math.round(scaleSmartParam(base.dehaze ?? 0, 0, 40, toneScale)),
      vibrance: Math.round(scaleSmartParam(base.vibrance ?? 0, 0, 65, colScale)),
      saturation: Math.round(scaleSmartParam(base.saturation ?? 0, 0, 30, colScale)),
      temperature: Math.round(scaleSmartParam(base.temperature ?? 0, 0, 35, colScale)),
      tint: Math.round(scaleSmartParam(base.tint ?? 0, 0, 35, colScale)),
      hsl: base.hsl
        ? {
            ...(prev.hsl || ({} as any)),
            ...base.hsl,
            oranges: base.hsl.oranges
              ? {
                  hue: Math.round((base.hsl.oranges.hue || 0) * (sknScale / 100)),
                  saturation: Math.round((base.hsl.oranges.saturation || 0) * (sknScale / 100)),
                  luminance: Math.round((base.hsl.oranges.luminance || 0) * (sknScale / 100)),
                }
              : prev.hsl?.oranges,
          }
        : prev.hsl,
    }));
  };

  const handleQuickCorrectNudge = (channel: 'S' | 'E' | 'R' | 'G' | 'B', dir: 1 | -1) => {
    setAdjustments((prev: Partial<Adjustments>) => {
      if (channel === 'S') return { ...prev, vibrance: Math.max(-100, Math.min(100, (prev.vibrance || 0) + 5 * dir)) };
      if (channel === 'E')
        return { ...prev, exposure: Number(Math.max(-5, Math.min(5, (prev.exposure || 0) + 0.1 * dir)).toFixed(2)) };
      if (channel === 'R')
        return {
          ...prev,
          temperature: Math.max(-100, Math.min(100, (prev.temperature || 0) + 3 * dir)),
          tint: Math.max(-100, Math.min(100, (prev.tint || 0) + 2 * dir)),
        };
      if (channel === 'G') return { ...prev, tint: Math.max(-100, Math.min(100, (prev.tint || 0) - 3 * dir)) };
      return { ...prev, temperature: Math.max(-100, Math.min(100, (prev.temperature || 0) - 3 * dir)) };
    });
  };

  const handleWeddingMagic = (scene: WeddingScene = selectedWeddingScene) => {
    setSelectedWeddingScene(scene);
    setIsWeddingMenuOpen(false);

    setAdjustments((prev: Partial<Adjustments>) => {
      let expShift = 0.12;
      let contrast = 14;
      let highlights = -24;
      let whites = 12;
      let shadows = 18;
      let blacks = -8;
      let tempShift = 3;
      let tintShift = 2;
      let clarity = -6;
      let sharpness = 32;
      let halation = 14;
      let colorDenoise = prev.colorNoiseReduction || 0;
      let lumaDenoise = prev.lumaNoiseReduction || 0;
      let grainAmount = prev.grainAmount || 0;
      let grainSize = prev.grainSize || 1.0;
      let grainRoughness = prev.grainRoughness || 20;

      let orangeHsl = { hue: 2, saturation: -8, luminance: 12 };
      let redHsl = { hue: 4, saturation: -4, luminance: 4 };
      let greenHsl = { hue: 22, saturation: -28, luminance: -8 };
      let yellowHsl = { hue: -8, saturation: -10, luminance: 6 };
      const blueHsl =
        scene === 'goldenHour' ? { hue: 4, saturation: 14, luminance: -6 } : { hue: 6, saturation: 10, luminance: -4 };

      let highlightGrading = { hue: 42, saturation: 18, luminance: 6 };
      let shadowGrading = { hue: 215, saturation: 12, luminance: -4 };

      let blueCalibration = 12;
      let redCalibration = 4;

      if (scene === 'bridalGlow') {
        expShift = 0.22;
        contrast = 10;
        highlights = -18;
        whites = 16;
        shadows = 22;
        blacks = -4;
        tempShift = 4;
        clarity = -8;
        sharpness = 35;
        halation = 18;
        orangeHsl = { hue: 3, saturation: -6, luminance: 16 };
        redHsl = { hue: 4, saturation: -4, luminance: 6 };
        greenHsl = { hue: 20, saturation: -24, luminance: -6 };
        highlightGrading = { hue: 38, saturation: 16, luminance: 8 };
        blueCalibration = 14;
      } else if (scene === 'ceremony') {
        expShift = 0.08;
        contrast = 16;
        highlights = -32;
        whites = 14;
        shadows = 16;
        blacks = -10;
        tempShift = -2;
        tintShift = 3;
        clarity = -4;
        sharpness = 30;
        lumaDenoise = 12;
        colorDenoise = 20;
        halation = 10;
        orangeHsl = { hue: 2, saturation: -10, luminance: 10 };
        redHsl = { hue: 6, saturation: -6, luminance: 2 };
        yellowHsl = { hue: -12, saturation: -14, luminance: 4 };
        blueCalibration = 10;
      } else if (scene === 'goldenHour') {
        expShift = 0.16;
        contrast = 18;
        highlights = -22;
        whites = 10;
        shadows = 18;
        blacks = -8;
        tempShift = 6;
        clarity = -6;
        sharpness = 32;
        halation = 16;
        orangeHsl = { hue: 2, saturation: -6, luminance: 12 };
        greenHsl = { hue: 26, saturation: -32, luminance: -10 };
        yellowHsl = { hue: -6, saturation: -8, luminance: 8 };
        highlightGrading = { hue: 44, saturation: 24, luminance: 6 };
        shadowGrading = { hue: 212, saturation: 14, luminance: -4 };
        blueCalibration = 16;
        redCalibration = 6;
      } else if (scene === 'reception') {
        expShift = 0.14;
        contrast = 20;
        highlights = -20;
        whites = 12;
        shadows = 14;
        blacks = -12;
        tempShift = 2;
        clarity = 4;
        sharpness = 28;
        colorDenoise = 36;
        lumaDenoise = 16;
        grainAmount = 16;
        grainSize = 1.2;
        grainRoughness = 35;
        halation = 8;
        highlightGrading = { hue: 40, saturation: 14, luminance: 4 };
        shadowGrading = { hue: 220, saturation: 10, luminance: -6 };
      }

      return {
        ...prev,
        exposure: (prev.exposure || 0) + expShift,
        contrast,
        highlights,
        whites,
        shadows,
        blacks,
        temperature: (prev.temperature || 0) + tempShift,
        tint: (prev.tint || 0) + tintShift,
        clarity,
        sharpness,
        toneMapper: 'agx',
        halationAmount: halation,
        colorNoiseReduction: colorDenoise,
        lumaNoiseReduction: lumaDenoise,
        grainAmount,
        grainSize,
        grainRoughness,
        hsl: {
          ...(prev.hsl || ({} as any)),
          oranges: orangeHsl,
          reds: redHsl,
          greens: greenHsl,
          yellows: yellowHsl,
          blues: blueHsl,
        } as any,
        colorGrading: {
          ...(prev.colorGrading || ({} as any)),
          highlights: highlightGrading,
          shadows: shadowGrading,
        } as any,
        colorCalibration: {
          ...(prev.colorCalibration || ({} as any)),
          blueHue: blueCalibration,
          redHue: redCalibration,
        } as any,
      };
    });
  };

  const handleMacroMagic = (scene: MacroScene = selectedMacroScene) => {
    setSelectedMacroScene(scene);
    setIsMacroMenuOpen(false);

    setAdjustments((prev: Partial<Adjustments>) => {
      let expShift = 0.08;
      let contrast = 18;
      let highlights = -38;
      let whites = 16;
      let shadows = 14;
      let blacks = -6;
      let tempShift = 1;
      const tintShift = 0;
      let clarity = 16;
      let structure = 18;
      let sharpness = 42;
      let sharpnessThreshold = 55;
      let caRedCyan = -8;
      let caBlueYellow = 6;
      let vibrance = 20;
      let saturation = 6;
      let halation = 0;
      const lumaDenoise = 10;
      const colorDenoise = 24;

      let yellowHsl = { hue: -4, saturation: 12, luminance: 10 };
      let greenHsl = { hue: 14, saturation: -12, luminance: -6 };
      let aquaHsl = { hue: 8, saturation: 14, luminance: 4 };
      let redHsl = { hue: 2, saturation: 8, luminance: 4 };
      const orangeHsl = { hue: 2, saturation: 6, luminance: 8 };

      let highlightGrading = (prev.colorGrading as any)?.highlights || { hue: 0, saturation: 0, luminance: 0 };
      let shadowGrading = (prev.colorGrading as any)?.shadows || { hue: 0, saturation: 0, luminance: 0 };

      if (scene === 'droplets') {
        expShift = 0.1;
        contrast = 20;
        highlights = -44;
        whites = 18;
        shadows = 16;
        blacks = -8;
        clarity = 22;
        structure = 20;
        sharpness = 40;
        sharpnessThreshold = 50;
        caRedCyan = -12;
        caBlueYellow = 8;
        aquaHsl = { hue: 10, saturation: 22, luminance: 6 };
        greenHsl = { hue: 16, saturation: -10, luminance: -4 };
      } else if (scene === 'insects') {
        expShift = 0.06;
        contrast = 22;
        highlights = -36;
        whites = 14;
        shadows = 18;
        blacks = -10;
        clarity = 20;
        structure = 24;
        sharpness = 48;
        sharpnessThreshold = 60;
        caRedCyan = -10;
        caBlueYellow = 6;
        vibrance = 24;
        redHsl = { hue: 4, saturation: 12, luminance: 6 };
        yellowHsl = { hue: -2, saturation: 16, luminance: 12 };
      } else if (scene === 'botanical') {
        expShift = 0.12;
        contrast = 14;
        highlights = -32;
        whites = 14;
        shadows = 20;
        blacks = -4;
        tempShift = 4;
        clarity = 10;
        structure = 14;
        sharpness = 36;
        sharpnessThreshold = 45;
        halation = 12;
        vibrance = 24;
        saturation = 8;
        yellowHsl = { hue: -6, saturation: 18, luminance: 14 };
        redHsl = { hue: 2, saturation: 14, luminance: 8 };
        greenHsl = { hue: 18, saturation: -16, luminance: -8 };
      } else if (scene === 'jewelry') {
        expShift = 0.05;
        contrast = 26;
        highlights = -45;
        whites = 22;
        shadows = 10;
        blacks = -12;
        tempShift = -2;
        clarity = 26;
        structure = 22;
        sharpness = 46;
        sharpnessThreshold = 40;
        caRedCyan = -14;
        caBlueYellow = 10;
        highlightGrading = { hue: 210, saturation: 8, luminance: 4 };
        shadowGrading = { hue: 225, saturation: 14, luminance: -6 };
      }

      return {
        ...prev,
        exposure: (prev.exposure || 0) + expShift,
        contrast,
        highlights,
        whites,
        shadows,
        blacks,
        temperature: (prev.temperature || 0) + tempShift,
        tint: (prev.tint || 0) + tintShift,
        clarity,
        structure,
        sharpness,
        sharpnessThreshold,
        chromaticAberrationRedCyan: caRedCyan,
        chromaticAberrationBlueYellow: caBlueYellow,
        vibrance,
        saturation,
        toneMapper: 'agx',
        halationAmount: halation,
        colorNoiseReduction: colorDenoise,
        lumaNoiseReduction: lumaDenoise,
        hsl: {
          ...(prev.hsl || ({} as any)),
          yellows: yellowHsl,
          greens: greenHsl,
          aquas: aquaHsl,
          reds: redHsl,
          oranges: orangeHsl,
        } as any,
        colorGrading: {
          ...(prev.colorGrading || ({} as any)),
          highlights: highlightGrading,
          shadows: shadowGrading,
        } as any,
      };
    });
  };

  const handleNightSkyMagic = (scene: AstroScene = selectedAstroScene) => {
    setSelectedAstroScene(scene);
    setIsAstroMenuOpen(false);

    setAdjustments((prev: Partial<Adjustments>) => {
      let expShift = 0.25;
      let contrast = 24;
      let highlights = -18;
      let whites = 18;
      let shadows = 14;
      let blacks = -18;
      let tempShift = -8;
      let tintShift = 10;
      let clarity = 22;
      let dehaze = 24;
      let sharpness = 35;
      let sharpnessThreshold = 60;
      let lumaDenoise = 28;
      let colorDenoise = 46;

      let redHsl = { hue: -6, saturation: 18, luminance: 8 };
      let magentaHsl = { hue: 0, saturation: 15, luminance: 4 };
      const yellowHsl = (prev.hsl as any)?.yellows || { hue: 0, saturation: 0, luminance: 0 };
      let greenHsl = (prev.hsl as any)?.greens || { hue: 0, saturation: 0, luminance: 0 };
      const aquaHsl = (prev.hsl as any)?.aquas || { hue: 0, saturation: 0, luminance: 0 };
      const blueHsl = (prev.hsl as any)?.blues || { hue: 0, saturation: 0, luminance: 0 };
      const orangeHsl = (prev.hsl as any)?.oranges || { hue: 0, saturation: 0, luminance: 0 };

      const highlightGrading = { hue: 38, saturation: 22, luminance: 4 };
      let shadowGrading = { hue: 220, saturation: 16, luminance: -6 };

      if (scene === 'milkyWay') {
        expShift = 0.28;
        contrast = 28;
        highlights = -20;
        whites = 20;
        shadows = 16;
        blacks = -20;
        tempShift = -9;
        tintShift = 12;
        clarity = 26;
        dehaze = 26;
        sharpness = 38;
        sharpnessThreshold = 65;
        lumaDenoise = 32;
        colorDenoise = 52;
        redHsl = { hue: -8, saturation: 24, luminance: 10 };
        magentaHsl = { hue: 2, saturation: 20, luminance: 6 };
      } else if (scene === 'starryLandscape') {
        expShift = 0.2;
        contrast = 20;
        highlights = -22;
        whites = 16;
        shadows = 28;
        blacks = -14;
        tempShift = -7;
        tintShift = 8;
        clarity = 18;
        dehaze = 22;
        sharpness = 32;
        sharpnessThreshold = 55;
        lumaDenoise = 26;
        colorDenoise = 44;
      } else if (scene === 'deepSky') {
        expShift = 0.32;
        contrast = 26;
        highlights = -24;
        whites = 16;
        shadows = 18;
        blacks = -22;
        tempShift = -10;
        tintShift = 14;
        clarity = 24;
        dehaze = 28;
        sharpness = 40;
        sharpnessThreshold = 70;
        lumaDenoise = 36;
        colorDenoise = 58;
        redHsl = { hue: -10, saturation: 28, luminance: 12 };
        magentaHsl = { hue: 4, saturation: 24, luminance: 8 };
      } else if (scene === 'moonlitAurora') {
        expShift = 0.15;
        contrast = 18;
        highlights = -34;
        whites = 14;
        shadows = 16;
        blacks = -12;
        tempShift = -5;
        tintShift = 6;
        clarity = 16;
        dehaze = 18;
        sharpness = 30;
        sharpnessThreshold = 50;
        lumaDenoise = 24;
        colorDenoise = 40;
        greenHsl = { hue: 10, saturation: 22, luminance: 10 };
        magentaHsl = { hue: 0, saturation: 22, luminance: 8 };
        shadowGrading = { hue: 215, saturation: 14, luminance: -4 };
      }

      return {
        ...prev,
        exposure: (prev.exposure || 0) + expShift,
        contrast,
        highlights,
        whites,
        shadows,
        blacks,
        temperature: (prev.temperature || 0) + tempShift,
        tint: (prev.tint || 0) + tintShift,
        clarity,
        dehaze,
        sharpness,
        sharpnessThreshold,
        colorNoiseReduction: colorDenoise,
        lumaNoiseReduction: lumaDenoise,
        hsl: {
          ...(prev.hsl || ({} as any)),
          reds: redHsl,
          magentas: magentaHsl,
          yellows: yellowHsl,
          greens: greenHsl,
          aquas: aquaHsl,
          blues: blueHsl,
          oranges: orangeHsl,
        } as any,
        colorGrading: {
          ...(prev.colorGrading || ({} as any)),
          shadows: shadowGrading,
          highlights: highlightGrading,
        } as any,
      };
    });
  };

  const handleRemoveLightPollution = async () => {
    try {
      setIsAstroMenuOpen(false);
      await invoke(Invokes.RemoveActiveLightPollutionGradient, {
        preservePedestal: 0.02,
      });
      setAdjustments((prev: Partial<Adjustments>) => ({
        ...prev,
        exposure: prev.exposure || 0,
      }));
    } catch (err) {
      console.error('Failed to remove light pollution gradient:', err);
    }
  };

  const handleAutoTune = async () => {
    setIsAutoTuning(true);
    try {
      const res = await invoke<{
        report?: { overall_score?: number };
        recommended?: Partial<Adjustments>;
      }>(Invokes.RunAutoTuneQualityLoop);
      if (res?.recommended) {
        const current = useEditorStore.getState().adjustments;
        const next = { ...current, ...res.recommended };
        setAdjustments(res.recommended);
        useEditorStore.getState().pushHistory(next, 'Auto-Tune Quality Loop');
        const score = res.report?.overall_score;
        toast.success(
          typeof score === 'number'
            ? `Auto-Tune Quality Guard applied (${Math.round(score)}/100)`
            : 'Auto-Tune Quality Guard applied',
        );
      }
    } catch (err) {
      toast.error(`Auto-Tune failed: ${String(err)}`);
    } finally {
      setIsAutoTuning(false);
    }
  };

  const handleAdjustmentChange = (key: BasicAdjustment, value: any) => {
    const numericValue = parseFloat(value);
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: numericValue }));
  };

  const handleToneMapperChange = (mapper: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      toneMapper: mapper as 'basic' | 'agx' | 'legacy',
    }));
  };

  const hideTonemapper = isForMask || appSettings?.tonemapperOverrideEnabled;

  return (
    <div>
      {!isForMask && (
        <div className="mb-3 space-y-2">
          <div className="grid grid-cols-5 gap-1">
            <button
              type="button"
              onClick={handleAutoEnhance}
              disabled={isAutoAnalyzing}
              className="flex items-center justify-center gap-1 py-1.5 px-1 rounded-md bg-surface hover:bg-surface-secondary border border-border-color/50 text-text-primary hover:text-amber-400 font-medium text-[11px] min-w-0 transition-all shadow-xs disabled:opacity-50"
              title="1-Click Universal AI Polish: Detects scene, balances dynamic range, and auto-levels horizon."
            >
              {isAutoAnalyzing ? (
                <Loader2 size={12} className="animate-spin text-amber-400 shrink-0" />
              ) : (
                <Zap size={12} className="text-amber-400 shrink-0" />
              )}
              <span className="truncate">Auto</span>
            </button>

            <div className="relative min-w-0" ref={weddingMenuRef}>
              <div className="flex rounded-md bg-surface hover:bg-surface-secondary border border-border-color/50 transition-all shadow-xs group">
                <button
                  type="button"
                  onClick={() => handleWeddingMagic(selectedWeddingScene)}
                  className="flex-1 min-w-0 flex items-center justify-center gap-1 py-1.5 px-1 text-text-primary hover:text-pink-400 font-medium text-[11px]"
                  title="1-Click Bridal & Portrait Engine"
                >
                  <Heart size={11} className="text-pink-400 group-hover:scale-110 transition-transform shrink-0" />
                  <span className="truncate">Wed</span>
                </button>
                <button
                  type="button"
                  onClick={() => setIsWeddingMenuOpen(!isWeddingMenuOpen)}
                  className="px-0.5 border-l border-border-color/40 text-text-secondary hover:text-text-primary transition-colors flex items-center justify-center shrink-0"
                  title="Select Wedding Scene Preset"
                >
                  <ChevronDown size={10} className={clsx('transition-transform', isWeddingMenuOpen && 'rotate-180')} />
                </button>
              </div>
              {isWeddingMenuOpen && (
                <div className="absolute top-full left-0 mt-1 py-1 bg-[#18181b] border border-border-color/80 rounded-md shadow-2xl backdrop-blur-xl z-50 text-xs min-w-[150px]">
                  {(
                    [
                      { key: 'master', label: 'Master Wedding Look' },
                      { key: 'bridalGlow', label: 'Bridal Glow' },
                      { key: 'ceremony', label: 'Ceremony (Lace)' },
                      { key: 'goldenHour', label: 'Golden Hour' },
                      { key: 'reception', label: 'Reception (Film)' },
                    ] as const
                  ).map(({ key, label }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => handleWeddingMagic(key as WeddingScene)}
                      className={clsx(
                        'w-full flex items-center justify-between px-2.5 py-1.5 text-left hover:bg-surface/80 transition-colors',
                        selectedWeddingScene === key ? 'text-pink-400 font-semibold' : 'text-text-secondary',
                      )}
                    >
                      <span className="truncate">{label}</span>
                      {selectedWeddingScene === key && <Check size={12} className="shrink-0 text-pink-400 ml-1" />}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div className="relative min-w-0" ref={macroMenuRef}>
              <div className="flex rounded-md bg-surface hover:bg-surface-secondary border border-border-color/50 transition-all shadow-xs group">
                <button
                  type="button"
                  onClick={() => handleMacroMagic(selectedMacroScene)}
                  className="flex-1 min-w-0 flex items-center justify-center gap-1 py-1.5 px-1 text-text-primary hover:text-emerald-400 font-medium text-[11px]"
                  title="1-Click Macro & Close-up Engine"
                >
                  <Flower2 size={11} className="text-emerald-400 group-hover:scale-110 transition-transform shrink-0" />
                  <span className="truncate">Macro</span>
                </button>
                <button
                  type="button"
                  onClick={() => setIsMacroMenuOpen(!isMacroMenuOpen)}
                  className="px-0.5 border-l border-border-color/40 text-text-secondary hover:text-text-primary transition-colors flex items-center justify-center shrink-0"
                  title="Select Macro Scene Preset"
                >
                  <ChevronDown size={10} className={clsx('transition-transform', isMacroMenuOpen && 'rotate-180')} />
                </button>
              </div>
              {isMacroMenuOpen && (
                <div className="absolute top-full left-0 mt-1 py-1 bg-[#18181b] border border-border-color/80 rounded-md shadow-2xl backdrop-blur-xl z-50 text-xs min-w-[150px]">
                  {(
                    [
                      { key: 'master', label: 'Master Macro Look' },
                      { key: 'droplets', label: 'Water Droplets' },
                      { key: 'insects', label: 'Insects & Chitin' },
                      { key: 'botanical', label: 'Botanical & Flora' },
                      { key: 'jewelry', label: 'Jewelry & Rings' },
                    ] as const
                  ).map(({ key, label }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => handleMacroMagic(key as MacroScene)}
                      className={clsx(
                        'w-full flex items-center justify-between px-2.5 py-1.5 text-left hover:bg-surface/80 transition-colors',
                        selectedMacroScene === key ? 'text-emerald-400 font-semibold' : 'text-text-secondary',
                      )}
                    >
                      <span className="truncate">{label}</span>
                      {selectedMacroScene === key && <Check size={12} className="shrink-0 text-emerald-400 ml-1" />}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div className="relative min-w-0" ref={astroMenuRef}>
              <div className="flex rounded-md bg-surface hover:bg-surface-secondary border border-border-color/50 transition-all shadow-xs group">
                <button
                  type="button"
                  onClick={() => handleNightSkyMagic(selectedAstroScene)}
                  className="flex-1 min-w-0 flex items-center justify-center gap-1 py-1.5 px-1 text-text-primary hover:text-accent font-medium text-[11px]"
                  title="1-Click Astrophotography Engine"
                >
                  <Sparkles size={11} className="text-accent group-hover:scale-110 transition-transform shrink-0" />
                  <span className="truncate">Astro</span>
                </button>
                <button
                  type="button"
                  onClick={() => setIsAstroMenuOpen(!isAstroMenuOpen)}
                  className="px-0.5 border-l border-border-color/40 text-text-secondary hover:text-text-primary transition-colors flex items-center justify-center shrink-0"
                  title="Select Astro Scene Preset"
                >
                  <ChevronDown size={10} className={clsx('transition-transform', isAstroMenuOpen && 'rotate-180')} />
                </button>
              </div>
              {isAstroMenuOpen && (
                <div className="absolute top-full right-0 mt-1 py-1 bg-[#18181b] border border-border-color/80 rounded-md shadow-2xl backdrop-blur-xl z-50 text-xs min-w-[175px]">
                  {(
                    [
                      { key: 'master', label: 'Master Astro Look' },
                      { key: 'milkyWay', label: 'Milky Way Core' },
                      { key: 'starryLandscape', label: 'Starry Landscape' },
                      { key: 'deepSky', label: 'Deep Sky & Nebulae' },
                      { key: 'moonlitAurora', label: 'Moonlit & Aurora' },
                    ] as const
                  ).map(({ key, label }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => handleNightSkyMagic(key as AstroScene)}
                      className={clsx(
                        'w-full flex items-center justify-between px-2.5 py-1.5 text-left hover:bg-surface/80 transition-colors',
                        selectedAstroScene === key ? 'text-accent font-semibold' : 'text-text-secondary',
                      )}
                    >
                      <span className="truncate">{label}</span>
                      {selectedAstroScene === key && <Check size={12} className="shrink-0 text-accent ml-1" />}
                    </button>
                  ))}
                  <div className="border-t border-border-color/50 my-1" />
                  <button
                    type="button"
                    onClick={handleRemoveLightPollution}
                    className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-left hover:bg-surface/80 text-amber-400 font-medium transition-colors"
                    title="Extracts and subtracts 2D polynomial background light pollution gradient"
                  >
                    <span className="truncate">Remove Light Pollution</span>
                  </button>
                </div>
              )}
            </div>

            <div className="relative min-w-0" ref={skyMenuRef}>
              <div className="flex rounded-md bg-surface hover:bg-surface-secondary border border-border-color/50 transition-all shadow-xs group">
                <button
                  type="button"
                  onClick={() => handleSkySculpt(selectedSkyPreset)}
                  className="flex-1 min-w-0 flex items-center justify-center gap-1 py-1.5 px-1 text-text-primary hover:text-sky-400 font-medium text-[11px]"
                  title="AI Sky Sculptor"
                >
                  <Sun size={11} className="text-sky-400 group-hover:scale-110 transition-transform shrink-0" />
                  <span className="truncate">Sky</span>
                </button>
                <button
                  type="button"
                  onClick={() => setIsSkyMenuOpen(!isSkyMenuOpen)}
                  className="px-0.5 border-l border-border-color/40 text-text-secondary hover:text-text-primary transition-colors flex items-center justify-center shrink-0"
                  title="Select Sky Preset"
                >
                  <ChevronDown size={10} className={clsx('transition-transform', isSkyMenuOpen && 'rotate-180')} />
                </button>
              </div>
              {isSkyMenuOpen && (
                <div className="absolute top-full right-0 mt-1 py-1 bg-[#18181b] border border-border-color/80 rounded-md shadow-2xl backdrop-blur-xl z-50 text-xs min-w-[170px]">
                  {[
                    { key: 'polar', label: 'Deep Polar Blue' },
                    { key: 'sunset', label: 'Sunset Amber Glow' },
                    { key: 'stormy', label: 'Stormy Drama' },
                    { key: 'golden', label: 'Golden Hour Radiance' },
                    { key: 'gentle', label: 'Clean Horizon Dehaze' },
                  ].map(({ key, label }) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => handleSkySculpt(key)}
                      className={clsx(
                        'w-full flex items-center justify-between px-2.5 py-1.5 text-left hover:bg-surface/80 transition-colors',
                        selectedSkyPreset === key ? 'text-accent font-semibold' : 'text-text-secondary',
                      )}
                    >
                      <span className="truncate">{label}</span>
                      {selectedSkyPreset === key && <Check size={12} className="shrink-0 text-accent ml-1" />}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>

          <div className="p-2 rounded-md bg-card-active border border-border-color/50 flex flex-col gap-1.5">
            <div className="flex items-center justify-between gap-1 flex-wrap">
              <span className="text-[11px] font-semibold text-amber-300">
                {smartDiag
                  ? `AI Scene: ${smartDiag.sceneName}`
                  : detectedSceneBadge
                    ? `AI Scene: ${detectedSceneBadge}`
                    : 'Smart Editing Ready'}
              </span>
              <label className="flex items-center gap-1 text-[10px] text-text-secondary cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={autoAnalyzeOnOpen}
                  onChange={(e) => {
                    if (appSettings) {
                      handleSettingsChange({ ...appSettings, autoAnalyzeOnOpen: e.target.checked });
                    }
                  }}
                  className="accent-amber-400 rounded-xs w-3 h-3"
                />
                Auto-Analyze on Open
              </label>
            </div>

            {smartDiag && (
              <div className="grid grid-cols-3 gap-1 text-[10px]">
                <div className="px-1.5 py-0.5 rounded bg-surface text-emerald-300 truncate" title="Per-Pixel Exposure & Highlight Guard">
                  EV: {smartDiag.perPixelEvStatus}
                </div>
                <div className="px-1.5 py-0.5 rounded bg-surface text-sky-300 truncate" title="Midrange-Normalized Color Cast Removal">
                  WB: {smartDiag.tintCastStatus}
                </div>
                <div className="px-1.5 py-0.5 rounded bg-surface text-purple-300 truncate" title="Pre-Tone Noise Gate Status">
                  NR: {smartDiag.noiseGateStatus}
                </div>
              </div>
            )}

            <Slider
              label="Smart Strength (%)"
              min={0}
              max={200}
              step={1}
              defaultValue={100}
              value={masterStrength}
              onChange={(e: any) => {
                const v = parseInt(e.target.value, 10);
                setMasterStrength(v);
                applyCoupledSmartSliders(v, enhanceMacro, colorMacro, skinMacro);
              }}
              onDragStateChange={onDragStateChange}
            />

            <div className="pl-2 border-l-2 border-amber-500/30 flex flex-col gap-1">
              <Slider
                label="Enhance & Per-Pixel Tone"
                min={0}
                max={200}
                step={1}
                defaultValue={100}
                value={enhanceMacro}
                onChange={(e: any) => {
                  const v = parseInt(e.target.value, 10);
                  setEnhanceMacro(v);
                  applyCoupledSmartSliders(masterStrength, v, colorMacro, skinMacro);
                }}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                label="Rich Color & Cast Removal"
                min={0}
                max={200}
                step={1}
                defaultValue={100}
                value={colorMacro}
                onChange={(e: any) => {
                  const v = parseInt(e.target.value, 10);
                  setColorMacro(v);
                  applyCoupledSmartSliders(masterStrength, enhanceMacro, v, skinMacro);
                }}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                label="Skin & Portrait Protection"
                min={0}
                max={200}
                step={1}
                defaultValue={100}
                value={skinMacro}
                onChange={(e: any) => {
                  const v = parseInt(e.target.value, 10);
                  setSkinMacro(v);
                  applyCoupledSmartSliders(masterStrength, enhanceMacro, colorMacro, v);
                }}
                onDragStateChange={onDragStateChange}
              />
            </div>

            <div className="flex items-center justify-between gap-1 pt-1 border-t border-border-color/40">
              <span className="text-[10px] text-text-secondary">Quick Nudge:</span>
              {(['S', 'E', 'R', 'G', 'B'] as const).map((ch) => (
                <div key={ch} className="flex items-center rounded bg-surface border border-border-color/40 text-[10px]">
                  <button
                    type="button"
                    onClick={() => handleQuickCorrectNudge(ch, -1)}
                    className="px-1 py-0.5 hover:bg-surface-secondary text-text-secondary hover:text-text-primary"
                    title={`Decrease ${ch}`}
                  >
                    -
                  </button>
                  <span className="px-1 font-semibold text-text-primary">{ch}</span>
                  <button
                    type="button"
                    onClick={() => handleQuickCorrectNudge(ch, 1)}
                    className="px-1 py-0.5 hover:bg-surface-secondary text-text-secondary hover:text-text-primary"
                    title={`Increase ${ch}`}
                  >
                    +
                  </button>
                </div>
              ))}
            </div>
          </div>

          <ReferencePresetsRibbon
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            onDragStateChange={onDragStateChange}
          />
          <Button
            onClick={handleAutoTune}
            disabled={isAutoTuning}
            variant="secondary"
            className="w-full flex items-center justify-center gap-1.5 text-xs py-1.5"
          >
            <Sparkles size={14} className={isAutoTuning ? 'animate-pulse text-accent' : 'text-accent'} />
            {isAutoTuning
              ? t('adjustments.basic.autoTuning', 'Optimizing Tone & Halos...')
              : t('adjustments.basic.autoTuneQuality', 'Auto-Tune Quality Guard')}
          </Button>
        </div>
      )}
      {hideTonemapper ? (
        <Slider
          resetKey="exposure"
          label={t('adjustments.basic.exposure')}
          max={5}
          min={-5}
          onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Exposure, e.target.value)}
          speedEditId={BasicAdjustment.Exposure}
          step={0.01}
          value={adjustments.exposure}
          onDragStateChange={onDragStateChange}
        />
      ) : (
        <ToneMapperSwitch
          selectedMapper={adjustments.toneMapper || 'agx'}
          onMapperChange={handleToneMapperChange}
          exposureValue={adjustments.exposure}
          onExposureChange={(value) => handleAdjustmentChange(BasicAdjustment.Exposure, value)}
          onDragStateChange={onDragStateChange}
        />
      )}
      <Slider
        resetKey="contrast"
        label={t('adjustments.basic.contrast')}
        max={100}
        min={-100}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Contrast, e.target.value)}
        speedEditId={BasicAdjustment.Contrast}
        step={1}
        value={adjustments.contrast}
        onDragStateChange={onDragStateChange}
      />
      <Slider
        resetKey="highlights"
        label={t('adjustments.basic.highlights')}
        max={100}
        min={-100}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Highlights, e.target.value)}
        speedEditId={BasicAdjustment.Highlights}
        step={1}
        value={adjustments.highlights}
        onDragStateChange={onDragStateChange}
      />
      <Slider
        resetKey="shadows"
        label={t('adjustments.basic.shadows')}
        max={100}
        min={-100}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Shadows, e.target.value)}
        speedEditId={BasicAdjustment.Shadows}
        step={1}
        value={adjustments.shadows}
        onDragStateChange={onDragStateChange}
      />
      <Slider
        resetKey="whites"
        label={t('adjustments.basic.whites')}
        max={100}
        min={-100}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Whites, e.target.value)}
        speedEditId={BasicAdjustment.Whites}
        step={1}
        value={adjustments.whites}
        onDragStateChange={onDragStateChange}
      />
      <Slider
        resetKey="blacks"
        label={t('adjustments.basic.blacks')}
        max={100}
        min={-100}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Blacks, e.target.value)}
        speedEditId={BasicAdjustment.Blacks}
        step={1}
        value={adjustments.blacks}
        onDragStateChange={onDragStateChange}
      />
      <Slider
        resetKey="brightness"
        label={t('adjustments.basic.brightness')}
        max={5}
        min={-5}
        onChange={(e: any) => handleAdjustmentChange(BasicAdjustment.Brightness, e.target.value)}
        step={0.01}
        value={adjustments.brightness}
        onDragStateChange={onDragStateChange}
      />
      {!isForMask && (
        <div className="mt-3 space-y-2">
          <LightsSubPipeline
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            onDragStateChange={onDragStateChange}
            isForMask={isForMask}
            appSettings={appSettings}
            defaultExpanded={false}
          />
          <div className="rounded-lg border border-border-color/60 bg-bg-secondary/40 overflow-hidden">
            <button
              type="button"
              onClick={() => setIsSupercontrastOpen((prev) => !prev)}
              className="w-full flex items-center justify-between px-3 py-2 text-xs font-medium text-text-primary hover:bg-card-active/40 transition-colors cursor-pointer"
            >
              <span className="flex items-center gap-1.5">
                <span>{t('adjustments.basic.supercontrast', 'Supercontrast Curve')}</span>
                {hasSupercontrastActive && <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />}
              </span>
              <ChevronDown
                size={14}
                className={clsx('text-text-secondary transition-transform duration-200', isSupercontrastOpen && 'rotate-180')}
              />
            </button>
            {isSupercontrastOpen && (
              <div className="px-2 pb-2 pt-1 border-t border-border-color/40">
                <SupercontrastCurve
                  adjustments={adjustments}
                  setAdjustments={setAdjustments}
                  onDragStateChange={onDragStateChange}
                />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default memo(BasicAdjustments, areSectionPropsEqual('basic'));
