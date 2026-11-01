import React, { useState, useCallback } from 'react';
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
  Palette,
  Flame,
  Camera,
  Film,
  Compass,
} from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { useEditorStore } from '../../../store/useEditorStore';
import { useEditorActions } from '../../../hooks/useEditorActions';
import { Adjustments } from '../../../utils/adjustments';
import { SNS_HDR_PROFILES } from '../../adjustments/ReferencePresetsRibbon';
import Slider from '../../ui/Slider';
import Text from '../../ui/Text';
import { TextVariants, TextWeights } from '../../../types/typography';

export interface CreativeLook {
  id: string;
  name: string;
  tag: string;
  description: string;
  gradient: string;
  iconColor: string;
  Icon: React.ElementType;
  adjustments: Partial<Adjustments>;
}

export const STUDIO_CREATIVE_LOOKS: CreativeLook[] = [
  {
    id: 'radiant-vivid',
    name: 'Vivid Punch',
    tag: 'Punch & Vibrance',
    description: 'Instant micro-contrast boost and dynamic color separation for punchy pop',
    gradient: 'from-amber-500/30 via-rose-500/20 to-orange-500/30',
    iconColor: 'text-amber-400',
    Icon: Flame,
    adjustments: {
      contrast: 15.0,
      highlights: -18.0,
      shadows: 24.0,
      clarity: 15.0,
      structure: 20.0,
      vibrance: 22.0,
      saturation: 8.0,
      lightsBrightness: 0.05,
      lightsContrast: 0.15,
      lightsDetails: 0.25,
      supercontrastMidtones: 0.15,
      supercontrastHighlights: 0.18,
    },
  },
  {
    id: 'radiant-portrait-glow',
    name: 'Portrait Glow',
    tag: 'Silky Skin & Orton',
    description: 'Gentle diffusion on skin tones with protected eye crispness and warm ambient glow',
    gradient: 'from-pink-500/30 via-rose-400/20 to-amber-400/30',
    iconColor: 'text-pink-400',
    Icon: Feather,
    adjustments: {
      contrast: -8.0,
      highlights: -12.0,
      shadows: 16.0,
      clarity: -10.0,
      structure: -12.0,
      glowAmount: 22.0,
      ortonSpread: 0.35,
      skinShield: 1.0,
      lightsTemperature: 0.06,
      lightsVibrance: 0.08,
      supercontrastShadows: -0.05,
    },
  },
  {
    id: 'radiant-cinematic-teal-orange',
    name: 'Cinematic Amber',
    tag: 'Film Split-Tone',
    description: 'Hollywood block-buster color grading with rich shadows and luminous warm skin',
    gradient: 'from-cyan-600/30 via-teal-600/20 to-amber-600/30',
    iconColor: 'text-cyan-400',
    Icon: Film,
    adjustments: {
      contrast: 18.0,
      highlights: -22.0,
      shadows: 20.0,
      whites: 8.0,
      blacks: -12.0,
      clarity: 12.0,
      structure: 18.0,
      lightsTemperature: 0.08,
      lightsSaturation: 0.06,
      supercontrastShadows: 0.22,
      supercontrastHighlights: 0.25,
    },
  },
  {
    id: 'radiant-golden-hour',
    name: 'Golden Hour',
    tag: 'Warm Sunlit Haze',
    description: 'Rich low-angle sunlight tones, deep amber glow and lifted warm shadow details',
    gradient: 'from-yellow-600/30 via-amber-600/20 to-red-600/30',
    iconColor: 'text-yellow-400',
    Icon: Sun,
    adjustments: {
      temperature: 8.0,
      tint: 2.0,
      contrast: 6.0,
      highlights: -25.0,
      shadows: 28.0,
      clarity: 8.0,
      vibrance: 16.0,
      lightsTemperature: 0.15,
      lightsVibrance: 0.12,
      glowAmount: 14.0,
    },
  },
  {
    id: 'radiant-nordic-cool',
    name: 'Nordic Clean',
    tag: 'Cool Crisp Air',
    description: 'Crisp mountain clarity, desaturated warm hues, and airy azure highlights',
    gradient: 'from-sky-600/30 via-blue-600/20 to-indigo-600/30',
    iconColor: 'text-sky-400',
    Icon: Compass,
    adjustments: {
      temperature: -6.0,
      contrast: 12.0,
      highlights: -15.0,
      shadows: 18.0,
      clarity: 22.0,
      structure: 28.0,
      saturation: -12.0,
      vibrance: 10.0,
      lightsContrast: 0.20,
    },
  },
  {
    id: 'radiant-mono-drama',
    name: 'Deep Monochrome',
    tag: 'Silver Halide B&W',
    description: 'High-contrast fine-art black and white with deep shadow densities and specular whites',
    gradient: 'from-zinc-700/40 via-stone-800/30 to-slate-900/40',
    iconColor: 'text-slate-300',
    Icon: Camera,
    adjustments: {
      saturation: -100.0,
      vibrance: -100.0,
      contrast: 30.0,
      highlights: -35.0,
      shadows: 35.0,
      whites: 25.0,
      blacks: -25.0,
      clarity: 32.0,
      structure: 45.0,
      supercontrastShadows: 0.35,
      supercontrastHighlights: 0.40,
    },
  },
];

export default function TonesPanel() {
  const { t } = useTranslation();
  const adjustments = useEditorStore((state) => state.adjustments);
  const { setAdjustments } = useEditorActions();

  const [activeProfileId, setActiveProfileId] = useState<string | null>(null);
  const [profileIntensity, setProfileIntensity] = useState<number>(100);
  const [baselineAdjustments, setBaselineAdjustments] = useState<Partial<Adjustments> | null>(null);

  const handleApplyPreset = useCallback(
    (presetId: string, presetAdjustments: Partial<Adjustments>) => {
      if (activeProfileId === presetId) {
        // Toggle off
        if (baselineAdjustments) {
          setAdjustments((prev) => ({ ...prev, ...baselineAdjustments }));
        }
        setActiveProfileId(null);
        setBaselineAdjustments(null);
        return;
      }

      let baseline = baselineAdjustments;
      if (!baseline) {
        const keys = Object.keys(presetAdjustments) as (keyof Adjustments)[];
        const backup: any = {};
        for (const k of keys) {
          backup[k] = adjustments[k];
        }
        baseline = backup;
        setBaselineAdjustments(backup);
      }

      setActiveProfileId(presetId);
      const factor = profileIntensity / 100.0;
      const blended: any = {};
      for (const [k, v] of Object.entries(presetAdjustments)) {
        const baseVal = (baseline as any)[k] ?? 0;
        if (typeof v === 'number') {
          blended[k] = Number((baseVal + (v - baseVal) * factor).toFixed(3));
        } else {
          blended[k] = v;
        }
      }

      setAdjustments((prev) => ({
        ...prev,
        ...blended,
      }));
    },
    [activeProfileId, baselineAdjustments, adjustments, profileIntensity, setAdjustments],
  );

  const handleIntensityChange = useCallback(
    (newIntensity: number) => {
      setProfileIntensity(newIntensity);
      if (!activeProfileId || !baselineAdjustments) return;

      const profile =
        SNS_HDR_PROFILES.find((p) => p.id === activeProfileId) ||
        STUDIO_CREATIVE_LOOKS.find((l) => l.id === activeProfileId);
      if (!profile) return;

      const factor = newIntensity / 100.0;
      const blended: any = {};
      for (const [k, v] of Object.entries(profile.adjustments)) {
        const baseVal = (baselineAdjustments as any)[k] ?? 0;
        if (typeof v === 'number') {
          blended[k] = Number((baseVal + (v - baseVal) * factor).toFixed(3));
        } else {
          blended[k] = v;
        }
      }

      setAdjustments((prev) => ({
        ...prev,
        ...blended,
      }));
    },
    [activeProfileId, baselineAdjustments, setAdjustments],
  );

  const handleReset = useCallback(() => {
    if (baselineAdjustments) {
      setAdjustments((prev) => ({ ...prev, ...baselineAdjustments }));
    }
    setActiveProfileId(null);
    setBaselineAdjustments(null);
    setProfileIntensity(100);
  }, [baselineAdjustments, setAdjustments]);

  return (
    <div className="w-full h-full flex flex-col bg-bg-secondary select-none overflow-hidden">
      {/* Panel Header */}
      <div className="p-3 border-b border-surface flex items-center justify-between shrink-0">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-accent/10 text-accent">
            <Palette size={16} />
          </div>
          <div>
            <Text variant={TextVariants.heading} weight={TextWeights.semibold} className="text-sm">
              {t('editor.tones.title', 'Tones & LOOKs')}
            </Text>
            <span className="text-[10px] text-text-secondary leading-none block">
              {t('editor.tones.subtitle', 'Instant HDR & Studio Styles')}
            </span>
          </div>
        </div>

        {activeProfileId && (
          <button
            onClick={handleReset}
            className="flex items-center gap-1 text-[11px] px-2 py-0.5 rounded bg-surface hover:bg-card-active text-text-secondary hover:text-text-primary transition-colors"
            title="Reset tone profile"
          >
            <RotateCcw size={11} />
            <span>Reset</span>
          </button>
        )}
      </div>

      {/* Profile Intensity Slider when active */}
      {activeProfileId && (
        <div className="px-3 py-2 bg-surface/40 border-b border-surface shrink-0">
          <div className="flex items-center justify-between text-[11px] mb-1">
            <span className="text-text-secondary flex items-center gap-1">
              <Sliders size={12} />
              {t('editor.tones.intensity', 'Profile Amount')}
            </span>
            <span className="font-mono text-accent text-[11px]">{profileIntensity}%</span>
          </div>
          <Slider
            label=""
            min={0}
            max={150}
            step={1}
            value={profileIntensity}
            onChange={(e: any) => handleIntensityChange(Number(e.target.value))}
          />
        </div>
      )}

      {/* Scrollable Profiles List */}
      <div className="flex-1 overflow-y-auto custom-scrollbar p-3 space-y-4">
        {/* Section 1: Dynamic HDR Profiles */}
        <div>
          <div className="flex items-center gap-1.5 mb-2">
            <Sparkles size={13} className="text-emerald-400" />
            <Text variant={TextVariants.label} weight={TextWeights.semibold} className="text-xs uppercase tracking-wider text-text-secondary">
              Dynamic HDR Profiles
            </Text>
          </div>

          <div className="grid grid-cols-2 gap-2">
            {SNS_HDR_PROFILES.map((profile) => {
              const isActive = activeProfileId === profile.id;
              const { Icon } = profile;

              return (
                <button
                  key={profile.id}
                  onClick={() => handleApplyPreset(profile.id, profile.adjustments)}
                  className={clsx(
                    'relative flex flex-col p-2.5 rounded-lg border text-left transition-all',
                    'bg-linear-to-br',
                    profile.gradient,
                    isActive
                      ? 'border-emerald-400 ring-2 ring-emerald-400/30 scale-[1.02] shadow-sm'
                      : 'border-surface/70 hover:border-surface hover:scale-[1.01] hover:brightness-110',
                  )}
                >
                  {isActive && (
                    <div className="absolute top-1.5 right-1.5 w-3.5 h-3.5 rounded-full bg-emerald-500 text-white flex items-center justify-center">
                      <Check size={9} strokeWidth={3} />
                    </div>
                  )}
                  <div className="flex items-center gap-2 mb-1.5">
                    <div className={clsx('p-1 rounded bg-surface/60', profile.iconColor)}>
                      <Icon size={14} />
                    </div>
                    <span className="font-semibold text-xs text-text-primary leading-tight">
                      {profile.name}
                    </span>
                  </div>
                  <span className="text-[10px] text-text-secondary font-medium leading-tight">
                    {profile.tag}
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Section 2: Studio Creative Looks */}
        <div>
          <div className="flex items-center gap-1.5 mb-2">
            <Flame size={13} className="text-amber-400" />
            <Text variant={TextVariants.label} weight={TextWeights.semibold} className="text-xs uppercase tracking-wider text-text-secondary">
              Studio Creative Looks
            </Text>
          </div>

          <div className="grid grid-cols-2 gap-2">
            {STUDIO_CREATIVE_LOOKS.map((look) => {
              const isActive = activeProfileId === look.id;
              const { Icon } = look;

              return (
                <button
                  key={look.id}
                  onClick={() => handleApplyPreset(look.id, look.adjustments)}
                  className={clsx(
                    'relative flex flex-col p-2.5 rounded-lg border text-left transition-all',
                    'bg-linear-to-br',
                    look.gradient,
                    isActive
                      ? 'border-amber-400 ring-2 ring-amber-400/30 scale-[1.02] shadow-sm'
                      : 'border-surface/70 hover:border-surface hover:scale-[1.01] hover:brightness-110',
                  )}
                >
                  {isActive && (
                    <div className="absolute top-1.5 right-1.5 w-3.5 h-3.5 rounded-full bg-amber-500 text-white flex items-center justify-center">
                      <Check size={9} strokeWidth={3} />
                    </div>
                  )}
                  <div className="flex items-center gap-2 mb-1.5">
                    <div className={clsx('p-1 rounded bg-surface/60', look.iconColor)}>
                      <Icon size={14} />
                    </div>
                    <span className="font-semibold text-xs text-text-primary leading-tight">
                      {look.name}
                    </span>
                  </div>
                  <span className="text-[10px] text-text-secondary font-medium leading-tight">
                    {look.tag}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
