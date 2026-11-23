import React, { useState, useRef, useCallback, useMemo, useEffect } from 'react';
import { RotateCcw, Sliders, Waves, Activity } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import Slider from '../ui/Slider';
import Text from '../ui/Text';
import { TextVariants, TextWeights } from '../../types/typography';
import { Adjustments } from '../../utils/adjustments';

export interface SupercontrastCurveProps {
  adjustments: Adjustments;
  setAdjustments: (adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  theme?: string;
}

const WIDTH = 260;
const HEIGHT = 150;
const PADDING = 10;
const GRAPH_W = WIDTH - PADDING * 2;
const GRAPH_H = HEIGHT - PADDING * 2;

export default function SupercontrastCurve({
  adjustments,
  setAdjustments,
  onDragStateChange,
}: SupercontrastCurveProps) {
  const { t } = useTranslation();
  const svgRef = useRef<SVGSVGElement | null>(null);
  const [activeThumb, setActiveThumb] = useState<'shadows' | 'midtones' | 'highlights' | 'pivots' | null>(null);

  // Normalized values [-1.0, 1.0] from adjustments
  const scShadows = useMemo(() => {
    const v = adjustments.supercontrastShadows ?? 0;
    return Math.abs(v) > 1.5 ? v / 100 : v;
  }, [adjustments.supercontrastShadows]);

  const scMidtones = useMemo(() => {
    const v = adjustments.supercontrastMidtones ?? 0;
    return Math.abs(v) > 1.5 ? v / 100 : v;
  }, [adjustments.supercontrastMidtones]);

  const scHighlights = useMemo(() => {
    const v = adjustments.supercontrastHighlights ?? 0;
    return Math.abs(v) > 1.5 ? v / 100 : v;
  }, [adjustments.supercontrastHighlights]);

  const scPivots = useMemo(() => {
    const v = adjustments.supercontrastPivots ?? 0;
    return Math.abs(v) > 1.5 ? v / 100 : v;
  }, [adjustments.supercontrastPivots]);

  // Optical Hermite Pivot Locations matching WGSL shader
  const pS = useMemo(() => Math.max(0.05, Math.min(0.40, 0.18 + 0.10 * scPivots)), [scPivots]);
  const pM = 0.50;
  const pH = useMemo(() => Math.max(0.60, Math.min(0.95, 0.82 - 0.10 * scPivots)), [scPivots]);

  // Mathematical cubic Hermite transfer function evaluated at luma in [0, 1]
  const evaluateLuma = useCallback(
    (luma: number): number => {
      let deltaS = 0.0;
      if (luma < pM) {
        const tVal = Math.max(0, Math.min(1, luma / (pS * 2.0)));
        const hermiteS = tVal * tVal * (3.0 - 2.0 * tVal);
        deltaS = (hermiteS - tVal) * scShadows * 0.45;
      }

      let deltaH = 0.0;
      if (luma > pM) {
        const tVal = Math.max(0, Math.min(1, (luma - (2.0 * pH - 1.0)) / (2.0 * (1.0 - pH))));
        const hermiteH = tVal * tVal * (3.0 - 2.0 * tVal);
        deltaH = (hermiteH - tVal) * scHighlights * 0.45;
      }

      const deltaM = Math.sin(luma * Math.PI) * scMidtones * 0.35;
      return Math.max(0, Math.min(1, luma + deltaS + deltaM + deltaH));
    },
    [pS, pM, pH, scShadows, scMidtones, scHighlights],
  );

  // Build SVG path points
  const { pathData, areaData, thumbPoints } = useMemo(() => {
    const samples = 64;
    const pts: { x: number; y: number }[] = [];

    for (let i = 0; i <= samples; i++) {
      const lumaIn = i / samples;
      const lumaOut = evaluateLuma(lumaIn);
      const px = PADDING + lumaIn * GRAPH_W;
      const py = PADDING + (1.0 - lumaOut) * GRAPH_H;
      pts.push({ x: px, y: py });
    }

    let pStr = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
    for (let i = 1; i < pts.length; i++) {
      pStr += ` L ${pts[i].x.toFixed(1)} ${pts[i].y.toFixed(1)}`;
    }

    const aStr = `${pStr} L ${(PADDING + GRAPH_W).toFixed(1)} ${(PADDING + GRAPH_H).toFixed(1)} L ${PADDING.toFixed(1)} ${(PADDING + GRAPH_H).toFixed(1)} Z`;

    const sY = PADDING + (1.0 - evaluateLuma(pS)) * GRAPH_H;
    const mY = PADDING + (1.0 - evaluateLuma(pM)) * GRAPH_H;
    const hY = PADDING + (1.0 - evaluateLuma(pH)) * GRAPH_H;

    return {
      pathData: pStr,
      areaData: aStr,
      thumbPoints: {
        shadows: { x: PADDING + pS * GRAPH_W, y: sY },
        midtones: { x: PADDING + pM * GRAPH_W, y: mY },
        highlights: { x: PADDING + pH * GRAPH_W, y: hY },
      },
    };
  }, [evaluateLuma, pS, pM, pH]);

  // Updates single parameter in adjustments store
  const updateParam = useCallback(
    (key: keyof Adjustments, floatVal: number) => {
      const clamped = Number(Math.max(-1.0, Math.min(1.0, floatVal)).toFixed(3));
      setAdjustments((prev: Adjustments) => {
        const next: any = {
          ...prev,
          [key]: clamped,
        };
        next.supercontrast = {
          ...(prev.supercontrast || {}),
          [key]: clamped,
        };
        return next;
      });
    },
    [setAdjustments],
  );

  // Dragging handlers for thumbs on the curve
  const handlePointerDownThumb = (thumb: 'shadows' | 'midtones' | 'highlights', e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setActiveThumb(thumb);
    onDragStateChange?.(true);
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!activeThumb || !svgRef.current) return;
    const rect = svgRef.current.getBoundingClientRect();
    const clientY = e.clientY - rect.top - PADDING;
    const normY = 1.0 - Math.max(0, Math.min(1, clientY / GRAPH_H));

    if (activeThumb === 'shadows') {
      const diff = normY - pS;
      updateParam('supercontrastShadows', diff * 4.0);
    } else if (activeThumb === 'midtones') {
      const diff = normY - pM;
      updateParam('supercontrastMidtones', diff * 3.0);
    } else if (activeThumb === 'highlights') {
      const diff = normY - pH;
      updateParam('supercontrastHighlights', diff * 4.0);
    }
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if (activeThumb) {
      try {
        (e.target as HTMLElement).releasePointerCapture?.(e.pointerId);
      } catch {
        // ignore pointer capture release error
      }
      setActiveThumb(null);
      onDragStateChange?.(false);
    }
  };

  const handleResetAll = useCallback(() => {
    setAdjustments((prev: Adjustments) => ({
      ...prev,
      supercontrastShadows: 0.0,
      supercontrastMidtones: 0.0,
      supercontrastHighlights: 0.0,
      supercontrastPivots: 0.0,
      supercontrast: {
        ...(prev.supercontrast || {}),
        supercontrastShadows: 0.0,
        supercontrastMidtones: 0.0,
        supercontrastHighlights: 0.0,
        supercontrastPivots: 0.0,
      },
    }));
  }, [setAdjustments]);

  const isModified = useMemo(
    () => Math.abs(scShadows) > 0.005 || Math.abs(scMidtones) > 0.005 || Math.abs(scHighlights) > 0.005 || Math.abs(scPivots) > 0.005,
    [scShadows, scMidtones, scHighlights, scPivots],
  );

  return (
    <div className="bg-bg-tertiary rounded-lg p-2.5 my-1.5 border border-surface/50 shadow-xs flex flex-col gap-2">
      {/* Header */}
      <div className="flex items-center justify-between select-none py-0.5">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-sky-500/10 text-sky-400">
            <Activity size={15} />
          </div>
          <div>
            <div className="flex items-center gap-1.5">
              <Text variant={TextVariants.body} weight={TextWeights.semibold} className="text-text-primary">
                {t('adjustments.supercontrast.title', 'Supercontrast')}
              </Text>
              {isModified && <span className="w-1.5 h-1.5 rounded-full bg-sky-400 ring-2 ring-sky-400/20" />}
            </div>
            <span className="text-[10px] text-text-secondary leading-tight block">
              {t('adjustments.supercontrast.desc', '3-band Hermite pivots for zero-halo tonal separation')}
            </span>
          </div>
        </div>

        {isModified && (
          <button
            onClick={handleResetAll}
            className="p-1 rounded hover:bg-surface text-text-secondary hover:text-text-primary transition-colors"
            title={t('adjustments.supercontrast.reset', 'Reset Supercontrast')}
          >
            <RotateCcw size={13} />
          </button>
        )}
      </div>

      {/* Interactive 3-Thumb Hermite Curve Display */}
      <div className="relative rounded-md overflow-hidden bg-bg-secondary/70 border border-surface/50 p-1 flex justify-center">
        <svg
          ref={svgRef}
          width={WIDTH}
          height={HEIGHT}
          className="select-none touch-none cursor-crosshair"
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerUp}
        >
          <defs>
            <linearGradient id="scAreaGrad" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#38bdf8" stopOpacity="0.30" />
              <stop offset="100%" stopColor="#38bdf8" stopOpacity="0.02" />
            </linearGradient>
            <linearGradient id="scLineGrad" x1="0%" y1="0%" x2="100%" y2="0%">
              <stop offset="0%" stopColor="#38bdf8" />
              <stop offset="50%" stopColor="#c084fc" />
              <stop offset="100%" stopColor="#fbbf24" />
            </linearGradient>
          </defs>

          {/* Background Grid & 45 degree linear identity line */}
          <line
            x1={PADDING}
            y1={PADDING + GRAPH_H}
            x2={PADDING + GRAPH_W}
            y2={PADDING}
            stroke="currentColor"
            strokeOpacity={0.15}
            strokeDasharray="3 3"
            strokeWidth={1}
          />

          {/* Pivot Region Zone Dividers */}
          <line
            x1={PADDING + pS * GRAPH_W}
            y1={PADDING}
            x2={PADDING + pS * GRAPH_W}
            y2={PADDING + GRAPH_H}
            stroke="#38bdf8"
            strokeOpacity={0.25}
            strokeDasharray="2 2"
            strokeWidth={1}
          />
          <line
            x1={PADDING + pM * GRAPH_W}
            y1={PADDING}
            x2={PADDING + pM * GRAPH_W}
            y2={PADDING + GRAPH_H}
            stroke="#c084fc"
            strokeOpacity={0.25}
            strokeDasharray="2 2"
            strokeWidth={1}
          />
          <line
            x1={PADDING + pH * GRAPH_W}
            y1={PADDING}
            x2={PADDING + pH * GRAPH_W}
            y2={PADDING + GRAPH_H}
            stroke="#fbbf24"
            strokeOpacity={0.25}
            strokeDasharray="2 2"
            strokeWidth={1}
          />

          {/* Shaded Area under Curve */}
          <path d={areaData} fill="url(#scAreaGrad)" />

          {/* Dynamic Hermite Curve Spline */}
          <path
            d={pathData}
            fill="none"
            stroke="url(#scLineGrad)"
            strokeWidth={2.2}
            strokeLinecap="round"
            strokeLinejoin="round"
          />

          {/* Thumb 1: Shadows Pivot Thumb */}
          <g
            className="cursor-ns-resize group/thumb"
            onPointerDown={(e) => handlePointerDownThumb('shadows', e)}
            onDoubleClick={() => updateParam('supercontrastShadows', 0)}
          >
            <circle
              cx={thumbPoints.shadows.x}
              cy={thumbPoints.shadows.y}
              r={7}
              fill="#38bdf8"
              fillOpacity={0.9}
              stroke="#ffffff"
              strokeWidth={1.5}
              className="drop-shadow-sm transition-transform group-hover/thumb:scale-125"
            />
            <circle cx={thumbPoints.shadows.x} cy={thumbPoints.shadows.y} r={14} fill="transparent" />
          </g>

          {/* Thumb 2: Midtones Center Thumb */}
          <g
            className="cursor-ns-resize group/thumb"
            onPointerDown={(e) => handlePointerDownThumb('midtones', e)}
            onDoubleClick={() => updateParam('supercontrastMidtones', 0)}
          >
            <circle
              cx={thumbPoints.midtones.x}
              cy={thumbPoints.midtones.y}
              r={7}
              fill="#c084fc"
              fillOpacity={0.9}
              stroke="#ffffff"
              strokeWidth={1.5}
              className="drop-shadow-sm transition-transform group-hover/thumb:scale-125"
            />
            <circle cx={thumbPoints.midtones.x} cy={thumbPoints.midtones.y} r={14} fill="transparent" />
          </g>

          {/* Thumb 3: Highlights Pivot Thumb */}
          <g
            className="cursor-ns-resize group/thumb"
            onPointerDown={(e) => handlePointerDownThumb('highlights', e)}
            onDoubleClick={() => updateParam('supercontrastHighlights', 0)}
          >
            <circle
              cx={thumbPoints.highlights.x}
              cy={thumbPoints.highlights.y}
              r={7}
              fill="#fbbf24"
              fillOpacity={0.9}
              stroke="#ffffff"
              strokeWidth={1.5}
              className="drop-shadow-sm transition-transform group-hover/thumb:scale-125"
            />
            <circle cx={thumbPoints.highlights.x} cy={thumbPoints.highlights.y} r={14} fill="transparent" />
          </g>
        </svg>

        {/* Floating Curve Labels */}
        <div className="absolute bottom-1 left-2 text-[9px] font-mono text-[#38bdf8] select-none">
          Shadows {Math.round(scShadows * 100)}%
        </div>
        <div className="absolute top-1 left-1/2 -translate-x-1/2 text-[9px] font-mono text-[#c084fc] select-none">
          Midtones {Math.round(scMidtones * 100)}%
        </div>
        <div className="absolute bottom-1 right-2 text-[9px] font-mono text-[#fbbf24] select-none">
          Highlights {Math.round(scHighlights * 100)}%
        </div>
      </div>

      {/* Precision Sliders */}
      <div className="grid grid-cols-1 gap-1 mt-1">
        <Slider
          label={
            <span className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full bg-[#38bdf8]" />
              {t('adjustments.supercontrast.shadows', 'Shadows Contrast')}
            </span>
          }
          min={-100}
          max={100}
          step={1}
          suffix="%"
          defaultValue={0}
          value={Math.round(scShadows * 100)}
          onChange={(e: any) => updateParam('supercontrastShadows', Number(e.target.value) / 100)}
          onDragStateChange={onDragStateChange}
        />

        <Slider
          label={
            <span className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full bg-[#c084fc]" />
              {t('adjustments.supercontrast.midtones', 'Midtones Contrast')}
            </span>
          }
          min={-100}
          max={100}
          step={1}
          suffix="%"
          defaultValue={0}
          value={Math.round(scMidtones * 100)}
          onChange={(e: any) => updateParam('supercontrastMidtones', Number(e.target.value) / 100)}
          onDragStateChange={onDragStateChange}
        />

        <Slider
          label={
            <span className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full bg-[#fbbf24]" />
              {t('adjustments.supercontrast.highlights', 'Highlights Contrast')}
            </span>
          }
          min={-100}
          max={100}
          step={1}
          suffix="%"
          defaultValue={0}
          value={Math.round(scHighlights * 100)}
          onChange={(e: any) => updateParam('supercontrastHighlights', Number(e.target.value) / 100)}
          onDragStateChange={onDragStateChange}
        />

        <Slider
          label={
            <span className="flex items-center gap-1.5 text-text-secondary">
              <Sliders size={12} />
              {t('adjustments.supercontrast.pivots', 'Pivot Balance')}
            </span>
          }
          min={-50}
          max={50}
          step={1}
          defaultValue={0}
          value={Math.round(scPivots * 100)}
          onChange={(e: any) => updateParam('supercontrastPivots', Number(e.target.value) / 100)}
          onDragStateChange={onDragStateChange}
        />
      </div>
    </div>
  );
}
