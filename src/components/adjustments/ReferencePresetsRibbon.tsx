import React, { useState, useCallback, useMemo } from 'react';
import {
  Sun,
  Building2,
  Trees,
  Layers,
  Moon,
  Zap,
  Feather,
  CircleDot,
  Check,
  RotateCcw,
  Sparkles,
  Sliders,
} from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { Adjustments } from '../../utils/adjustments';
import Text from '../ui/Text';
import { TextVariants, TextWeights } from '../../types/typography';
import Slider from '../ui/Slider';

export interface ReferenceProfile {
  id: string;
  name: string;
  tag: string;
  description: string;
  gradient: string;
  iconColor: string;
  Icon: any;
  adjustments: Partial<Adjustments>;
}

export const SNS_HDR_PROFILES: ReferenceProfile[] = [
  {
    id: 'sns-natural',
    name: 'Natural',
    tag: 'Balanced HDR',
    description: 'Balanced natural HDR tone mapping with zero boundary halos and faithful colorimetry',
    gradient: 'from-emerald-600/30 via-teal-600/20 to-sky-600/30',
    iconColor: 'text-emerald-400',
    Icon: Sun,
    adjustments: {
      contrast: 0.0,
      highlights: -15.0,
      shadows: 20.0,
      whites: 5.0,
      blacks: -5.0,
      clarity: 5.0,
      structure: 10.0,
      contrastCleaner: 1.0,
      lightsBrightness: 0.0,
      lightsContrast: 0.05,
      lightsDetails: 0.15,
      lightsTemperature: 0.0,
      lightsTint: 0.0,
      lightsSaturation: 0.0,
      lightsVibrance: 0.05,
      supercontrastShadows: 0.05,
      supercontrastMidtones: 0.10,
      supercontrastHighlights: 0.05,
      supercontrastPivots: 0.0,
      skinShield: 0.90,
      foliageEnhancer: 0.0,
      ortonSpread: 0.0,
    },
  },
  {
    id: 'sns-interior',
    name: 'Interior',
    tag: 'Window Balance',
    description: 'Real-estate & architectural interior exposure fusion balancing exterior window sunlight with warm indoor ambient light',
    gradient: 'from-amber-600/30 via-orange-600/20 to-yellow-600/30',
    iconColor: 'text-amber-400',
    Icon: Building2,
    adjustments: {
      contrast: -5.0,
      highlights: -40.0,
      shadows: 45.0,
      whites: -10.0,
      blacks: 5.0,
      clarity: 10.0,
      structure: 15.0,
      contrastCleaner: 1.25,
      lightsBrightness: -0.25,
      lightsContrast: 0.18,
      lightsDetails: 0.28,
      lightsTemperature: 0.12,
      lightsTint: 0.0,
      lightsSaturation: -0.05,
      lightsVibrance: 0.0,
      supercontrastShadows: 0.15,
      supercontrastMidtones: 0.08,
      supercontrastHighlights: 0.20,
      supercontrastPivots: 0.15,
      skinShield: 0.85,
      foliageEnhancer: 0.0,
      ortonSpread: 0.0,
    },
  },
  {
    id: 'sns-landscape',
    name: 'Landscape',
    tag: 'Foliage & Sky',
    description: 'Vivid nature and landscape profile with enhanced foliage greens, deep polarized sky, and rich surface micro-textures',
    gradient: 'from-green-600/30 via-emerald-600/20 to-teal-600/30',
    iconColor: 'text-green-400',
    Icon: Trees,
    adjustments: {
      contrast: 10.0,
      highlights: -20.0,
      shadows: 25.0,
      whites: 10.0,
      blacks: -8.0,
      clarity: 18.0,
      structure: 25.0,
      contrastCleaner: 1.10,
      foliageEnhancer: 0.35,
      vibrance: 15.0,
      saturation: 8.0,
      lightsBrightness: -0.08,
      lightsContrast: 0.12,
      lightsDetails: 0.30,
      lightsTemperature: 0.0,
      lightsTint: 0.0,
      lightsSaturation: 0.15,
      lightsVibrance: 0.12,
      supercontrastShadows: 0.18,
      supercontrastMidtones: 0.12,
      supercontrastHighlights: 0.22,
      supercontrastPivots: -0.05,
      skinShield: 0.80,
      ortonSpread: 0.0,
    },
  },
  {
    id: 'sns-ldr',
    name: 'LDR',
    tag: 'Clean Baseline',
    description: 'Low Dynamic Range tone mapping for single high-contrast exposures without shadow noise amplification',
    gradient: 'from-blue-600/30 via-indigo-600/20 to-cyan-600/30',
    iconColor: 'text-blue-400',
    Icon: Layers,
    adjustments: {
      contrast: 8.0,
      highlights: -18.0,
      shadows: 22.0,
      whites: 4.0,
      blacks: -4.0,
      clarity: 8.0,
      structure: 12.0,
      contrastCleaner: 0.90,
      lightsBrightness: 0.05,
      lightsContrast: 0.08,
      lightsDetails: 0.14,
      lightsTemperature: 0.0,
      lightsTint: 0.0,
      lightsSaturation: 0.02,
      lightsVibrance: 0.06,
      supercontrastShadows: 0.10,
      supercontrastMidtones: 0.18,
      supercontrastHighlights: 0.10,
      supercontrastPivots: 0.10,
      skinShield: 0.88,
      foliageEnhancer: 0.0,
      ortonSpread: 0.0,
    },
  },
  {
    id: 'sns-night',
    name: 'Night',
    tag: 'Deep Astro',
    description: 'Nocturnal cityscapes and astrophotography with deep inky blacks, highlight flare control, and delicate bloom',
    gradient: 'from-purple-900/40 via-indigo-900/30 to-slate-900/40',
    iconColor: 'text-purple-400',
    Icon: Moon,
    adjustments: {
      contrast: 12.0,
      highlights: -25.0,
      shadows: 15.0,
      whites: 12.0,
      blacks: -15.0,
      clarity: 12.0,
      structure: 18.0,
      contrastCleaner: 1.20,
      glowAmount: 12.0,
      ortonSpread: 0.18,
      lightsBrightness: -0.15,
      lightsContrast: 0.25,
      lightsDetails: 0.12,
      lightsTemperature: -0.05,
      lightsTint: 0.0,
      lightsSaturation: 0.10,
      lightsVibrance: 0.08,
      supercontrastShadows: -0.08,
      supercontrastMidtones: 0.05,
      supercontrastHighlights: 0.28,
      supercontrastPivots: -0.20,
      skinShield: 0.85,
      foliageEnhancer: 0.0,
    },
  },
  {
    id: 'sns-dramatic',
    name: 'Dramatic',
    tag: 'High Impact',
    description: 'Punchy high-impact tonal drama with deep shadows, intense micro-structure, and zero halo artifacts',
    gradient: 'from-rose-600/30 via-red-600/20 to-orange-600/30',
    iconColor: 'text-rose-400',
    Icon: Zap,
    adjustments: {
      contrast: 20.0,
      highlights: -30.0,
      shadows: 30.0,
      whites: 18.0,
      blacks: -18.0,
      clarity: 28.0,
      structure: 45.0,
      contrastCleaner: 1.50,
      vibrance: 18.0,
      saturation: -5.0,
      lightsBrightness: -0.10,
      lightsContrast: 0.28,
      lightsDetails: 0.42,
      lightsTemperature: 0.02,
      lightsTint: 0.0,
      lightsSaturation: 0.12,
      lightsVibrance: 0.16,
      supercontrastShadows: 0.32,
      supercontrastMidtones: 0.15,
      supercontrastHighlights: 0.36,
      supercontrastPivots: 0.0,
      skinShield: 0.75,
      foliageEnhancer: 0.0,
      ortonSpread: 0.0,
    },
  },
  {
    id: 'sns-soft',
    name: 'Soft',
    tag: 'Portrait & Glow',
    description: 'Ethereal portrait and dreamy landscape rendering with silky smooth skin tones, reduced grit, and gentle Orton glow',
    gradient: 'from-pink-500/30 via-rose-500/20 to-violet-500/30',
    iconColor: 'text-pink-400',
    Icon: Feather,
    adjustments: {
      contrast: -12.0,
      highlights: -10.0,
      shadows: 18.0,
      whites: -5.0,
      blacks: 4.0,
      clarity: -12.0,
      structure: -15.0,
      contrastCleaner: 0.80,
      glowAmount: 25.0,
      ortonSpread: 0.40,
      skinShield: 1.00,
      lightsBrightness: 0.08,
      lightsContrast: -0.05,
      lightsDetails: -0.08,
      lightsTemperature: 0.04,
      lightsTint: 0.02,
      lightsSaturation: -0.04,
      lightsVibrance: 0.05,
      supercontrastShadows: -0.10,
      supercontrastMidtones: -0.05,
      supercontrastHighlights: -0.12,
      supercontrastPivots: 0.05,
      foliageEnhancer: 0.0,
    },
  },
  {
    id: 'sns-neutral',
    name: 'Neutral',
    tag: 'Linear Reference',
    description: 'Reference zero-point linear baseline for color-accurate studio and scientific reproduction',
    gradient: 'from-slate-600/30 via-zinc-600/20 to-stone-600/30',
    iconColor: 'text-slate-300',
    Icon: CircleDot,
    adjustments: {
      contrast: 0.0,
      highlights: 0.0,
      shadows: 0.0,
      whites: 0.0,
      blacks: 0.0,
      clarity: 0.0,
      structure: 0.0,
      contrastCleaner: 1.0,
      lightsBrightness: 0.0,
      lightsContrast: 0.0,
      lightsDetails: 0.0,
      lightsTemperature: 0.0,
      lightsTint: 0.0,
      lightsSaturation: 0.0,
      lightsVibrance: 0.0,
      supercontrastShadows: 0.0,
      supercontrastMidtones: 0.0,
      supercontrastHighlights: 0.0,
      supercontrastPivots: 0.0,
      skinShield: 0.90,
      foliageEnhancer: 0.0,
      ortonSpread: 0.0,
    },
  },
];

export interface ReferencePresetsRibbonProps {
  adjustments: Adjustments;
  setAdjustments: (adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  className?: string;
}

export default function ReferencePresetsRibbon({
  adjustments,
  setAdjustments,
  onDragStateChange,
  className,
}: ReferencePresetsRibbonProps) {
  const { t } = useTranslation();
  const [activeProfileId, setActiveProfileId] = useState<string | null>(null);
  const [intensity, setIntensity] = useState<number>(100);
  const [previousAdjustments, setPreviousAdjustments] = useState<Partial<Adjustments> | null>(null);

  const handleApplyProfile = useCallback(
    (profile: ReferenceProfile) => {
      if (activeProfileId === profile.id) {
        // Toggle off / revert
        if (previousAdjustments) {
          setAdjustments((prev) => ({ ...prev, ...previousAdjustments }));
        }
        setActiveProfileId(null);
        setPreviousAdjustments(null);
        return;
      }

      // If switching from another profile, retain the original user baseline
      // Only capture baseline from adjustments if no preset was previously active
      let baseline = previousAdjustments;
      if (!baseline) {
        const keysToBackup = Object.keys(profile.adjustments) as (keyof Adjustments)[];
        const backup: any = {};
        for (const k of keysToBackup) {
          backup[k] = adjustments[k];
        }
        baseline = backup;
        setPreviousAdjustments(backup);
      }
      setActiveProfileId(profile.id);

      // Apply with current intensity interpolated against the baseline
      const factor = intensity / 100;
      const appliedAdj: any = {};
      for (const [k, v] of Object.entries(profile.adjustments)) {
        const baseVal = (baseline as any)[k] ?? 0;
        if (typeof v === 'number') {
          appliedAdj[k] = Number((baseVal + (v - baseVal) * factor).toFixed(3));
        } else {
          appliedAdj[k] = v;
        }
      }

      setAdjustments((prev) => ({
        ...prev,
        ...appliedAdj,
      }));
    },
    [activeProfileId, adjustments, intensity, previousAdjustments, setAdjustments],
  );

  const handleIntensityChange = useCallback(
    (newIntensity: number) => {
      setIntensity(newIntensity);
      if (!activeProfileId || !previousAdjustments) return;
      const profile = SNS_HDR_PROFILES.find((p) => p.id === activeProfileId);
      if (!profile) return;

      const factor = newIntensity / 100;
      const blendedAdj: any = {};

      for (const [k, v] of Object.entries(profile.adjustments)) {
        const baseVal = (previousAdjustments as any)[k] ?? 0;
        if (typeof v === 'number') {
          blendedAdj[k] = Number((baseVal + (v - baseVal) * factor).toFixed(3));
        } else {
          blendedAdj[k] = v;
        }
      }

      setAdjustments((prev) => ({
        ...prev,
        ...blendedAdj,
      }));
    },
    [activeProfileId, previousAdjustments, setAdjustments],
  );

  const handleReset = useCallback(() => {
    if (previousAdjustments) {
      setAdjustments((prev) => ({ ...prev, ...previousAdjustments }));
    }
    setActiveProfileId(null);
    setPreviousAdjustments(null);
    setIntensity(100);
  }, [previousAdjustments, setAdjustments]);

  return (
    <div
      className={clsx(
        'bg-bg-tertiary rounded-lg p-2.5 my-1.5 border border-surface/50 shadow-xs flex flex-col gap-2',
        className,
      )}
    >
      {/* Ribbon Title Bar */}
      <div className="flex items-center justify-between select-none">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-emerald-500/10 text-emerald-400">
            <Sparkles size={14} />
          </div>
          <div>
            <div className="flex items-center gap-1.5">
              <Text variant={TextVariants.body} weight={TextWeights.semibold} className="text-text-primary">
                Dynamic HDR Profiles
              </Text>
              {activeProfileId && (
                <span className="text-[10px] px-1.5 py-0.2 rounded-full bg-emerald-500/20 text-emerald-300 font-mono">
                  {SNS_HDR_PROFILES.find((p) => p.id === activeProfileId)?.name}
                </span>
              )}
            </div>
            <span className="text-[10px] text-text-secondary leading-tight block">
              1-click photographic reference looks with halo-free dynamic range
            </span>
          </div>
        </div>

        {activeProfileId && (
          <button
            onClick={handleReset}
            className="p-1 rounded hover:bg-surface text-text-secondary hover:text-text-primary transition-colors flex items-center gap-1 text-[11px]"
            title="Reset to pre-profile adjustments"
          >
            <RotateCcw size={12} />
            <span>Reset</span>
          </button>
        )}
      </div>

      {/* 8-Profile Thumbnail Horizontal Ribbon */}
      <div className="flex gap-1.5 overflow-x-auto pb-1 scrollbar-thin scrollbar-thumb-surface scrollbar-track-transparent snap-x">
        {SNS_HDR_PROFILES.map((profile) => {
          const isActive = activeProfileId === profile.id;
          const { Icon } = profile;

          return (
            <button
              key={profile.id}
              onClick={() => handleApplyProfile(profile)}
              title={`${profile.name}: ${profile.description}`}
              className={clsx(
                'group relative shrink-0 flex flex-col items-center justify-between w-[72px] h-[78px] rounded-lg p-1.5 transition-all select-none snap-start border text-left',
                'bg-linear-to-b',
                profile.gradient,
                isActive
                  ? 'border-emerald-400 ring-2 ring-emerald-400/30 scale-[1.02] shadow-md'
                  : 'border-surface/60 hover:border-surface hover:scale-[1.01] hover:brightness-110',
              )}
            >
              {/* Active Badge */}
              {isActive && (
                <div className="absolute top-1 right-1 w-3.5 h-3.5 rounded-full bg-emerald-500 text-white flex items-center justify-center shadow-xs">
                  <Check size={10} strokeWidth={3} />
                </div>
              )}

              {/* Icon Container */}
              <div
                className={clsx(
                  'w-7 h-7 rounded-md flex items-center justify-center transition-colors',
                  isActive ? 'bg-emerald-500/20 text-emerald-300' : 'bg-surface/50 text-text-primary',
                )}
              >
                <Icon size={16} className={profile.iconColor} />
              </div>

              {/* Profile Name & Tag */}
              <div className="w-full text-center mt-1">
                <span className="block text-[11px] font-semibold text-text-primary leading-tight truncate">
                  {profile.name}
                </span>
                <span className="block text-[9px] text-text-secondary leading-tight truncate">
                  {profile.tag}
                </span>
              </div>
            </button>
          );
        })}
      </div>

      {/* Intensity Blend Slider when a profile is active */}
      {activeProfileId && (
        <div className="pt-1.5 border-t border-surface/40 flex flex-col gap-1">
          <div className="flex items-center justify-between text-[11px] text-text-secondary mb-0.5">
            <span className="flex items-center gap-1 font-medium">
              <Sliders size={12} />
              Profile Blend Intensity
            </span>
            <span className="font-mono text-text-primary">{intensity}%</span>
          </div>
          <Slider
            label=""
            min={10}
            max={100}
            step={1}
            suffix="%"
            value={intensity}
            onChange={(e: any) => handleIntensityChange(Number(e.target.value))}
            onDragStateChange={onDragStateChange}
          />
        </div>
      )}
    </div>
  );
}
