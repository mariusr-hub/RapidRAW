import { useCallback, useEffect, useRef, useState } from 'react';
import type Konva from 'konva';
import type { KonvaEventObject } from 'konva/lib/Node';
import { ActiveChannel, Adjustments, Coord, getDefaultCurves } from '../utils/adjustments';
import { HoveredPixel, useEditorStore } from '../store/useEditorStore';
import { RenderSize } from './useImageRenderSize';
import { FINE_ADJUSTMENT_MULTIPLIER, hasFineAdjustmentModifier } from '../utils/fineAdjustment';

const MAX_SAMPLER_EDGE = 512;
const POINT_MATCH_TOLERANCE_X = 8;
const ENDPOINT_MATCH_TOLERANCE_X = 4;
const MAX_CURVE_POINTS = 16;
const DRAG_GAIN = 0.65;

interface TargetedCurveToolOptions {
  imageSrc: string | null;
  getCanvasPointer(stage: Konva.Stage): { x: number; y: number } | null;
  imageRenderSize: RenderSize;
  setAdjustments(fn: (prev: Adjustments) => Adjustments, label?: string): void;
}

const srgbToLinear = (c: number): number => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

const computeHoveredPixel = (r: number, g: number, b: number): HoveredPixel => {
  const luma = Math.round(0.2126 * r + 0.7152 * g + 0.0722 * b);
  const lr = srgbToLinear(r / 255);
  const lg = srgbToLinear(g / 255);
  const lb = srgbToLinear(b / 255);

  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);

  const okL = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const okA = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const okB = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;

  return { r, g, b, luma, okL, okA, okB };
};

export const evalMonotoneCurveY = (points: Coord[], x: number): number => {
  if (!points || points.length === 0) return x;
  if (points.length === 1) return points[0].y;
  const sorted = [...points].sort((a, b) => a.x - b.x);
  if (x <= sorted[0].x) return sorted[0].y;
  if (x >= sorted[sorted.length - 1].x) return sorted[sorted.length - 1].y;

  const n = sorted.length;
  const deltas: number[] = [];
  const ms: number[] = [];

  for (let i = 0; i < n - 1; i++) {
    const dx = sorted[i + 1].x - sorted[i].x;
    const dy = sorted[i + 1].y - sorted[i].y;
    deltas.push(dx === 0 ? (dy >= 0 ? 1e6 : -1e6) : dy / dx);
  }
  ms.push(deltas[0]);
  for (let i = 1; i < n - 1; i++) {
    ms.push(deltas[i - 1] * deltas[i] <= 0 ? 0 : (deltas[i - 1] + deltas[i]) * 0.5);
  }
  ms.push(deltas[n - 2]);

  for (let i = 0; i < n - 1; i++) {
    if (deltas[i] === 0) {
      ms[i] = 0;
      ms[i + 1] = 0;
    } else {
      const alpha = ms[i] / deltas[i];
      const beta = ms[i + 1] / deltas[i];
      const tau = alpha * alpha + beta * beta;
      if (tau > 9) {
        const scale = 3.0 / Math.sqrt(tau);
        ms[i] = scale * alpha * deltas[i];
        ms[i + 1] = scale * beta * deltas[i];
      }
    }
  }

  for (let i = 0; i < n - 1; i++) {
    const p0 = sorted[i];
    const p1 = sorted[i + 1];
    if (x <= p1.x) {
      const dx = Math.max(1e-6, p1.x - p0.x);
      const t = (x - p0.x) / dx;
      const t2 = t * t;
      const t3 = t2 * t;
      const h00 = 2 * t3 - 3 * t2 + 1;
      const h10 = t3 - 2 * t2 + t;
      const h01 = -2 * t3 + 3 * t2;
      const h11 = t3 - t2;
      return Math.max(0, Math.min(255, h00 * p0.y + h10 * dx * ms[i] + h01 * p1.y + h11 * dx * ms[i + 1]));
    }
  }
  return sorted[n - 1].y;
};

const channelInputValue = (pixel: HoveredPixel, channel: ActiveChannel): number => {
  switch (channel) {
    case ActiveChannel.Red:
      return pixel.r;
    case ActiveChannel.Green:
      return pixel.g;
    case ActiveChannel.Blue:
      return pixel.b;
    default:
      return pixel.luma;
  }
};

export function useTargetedCurveTool({
  imageSrc,
  getCanvasPointer,
  imageRenderSize,
  setAdjustments,
}: TargetedCurveToolOptions) {
  const isActive = useEditorStore((state) => state.isTargetedCurveActive);
  const setEditor = useEditorStore((state) => state.setEditor);
  const [isDragging, setIsDragging] = useState(false);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const rafRef = useRef<number | null>(null);
  const pendingUvRef = useRef<{ u: number | null; v: number | null }>({ u: null, v: null });
  const detachDragRef = useRef<(() => void) | null>(null);
  const isDraggingRef = useRef(false);

  useEffect(() => {
    if (!imageSrc) {
      canvasRef.current = null;
      ctxRef.current = null;
      return;
    }
    let cancelled = false;
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      if (cancelled || img.naturalWidth <= 0 || img.naturalHeight <= 0) return;
      const scale = Math.min(1, MAX_SAMPLER_EDGE / Math.max(img.naturalWidth, img.naturalHeight));
      const w = Math.max(1, Math.round(img.naturalWidth * scale));
      const h = Math.max(1, Math.round(img.naturalHeight * scale));
      const canvas = canvasRef.current ?? document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      if (!ctx) return;
      ctx.drawImage(img, 0, 0, w, h);
      canvasRef.current = canvas;
      ctxRef.current = ctx;
    };
    img.src = imageSrc;
    return () => {
      cancelled = true;
    };
  }, [imageSrc]);

  const sampleNormalized = useCallback((u: number, v: number): HoveredPixel | null => {
    const canvas = canvasRef.current;
    const ctx = ctxRef.current;
    if (!canvas || !ctx || u < 0 || u > 1 || v < 0 || v > 1) return null;
    const px = Math.min(canvas.width - 1, Math.max(0, Math.floor(u * canvas.width)));
    const py = Math.min(canvas.height - 1, Math.max(0, Math.floor(v * canvas.height)));
    try {
      const data = ctx.getImageData(px, py, 1, 1).data;
      return computeHoveredPixel(data[0], data[1], data[2]);
    } catch {
      return null;
    }
  }, []);

  const updateHoverAtNormalized = useCallback(
    (u: number | null, v: number | null) => {
      pendingUvRef.current = { u, v };
      if (rafRef.current !== null) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        const { u: curU, v: curV } = pendingUvRef.current;
        if (curU === null || curV === null) {
          const st = useEditorStore.getState();
          if (st.hoveredPixel !== null || st.targetedCurveHover !== null) {
            setEditor({ hoveredPixel: null, targetedCurveHover: null });
          }
          return;
        }
        const pixel = sampleNormalized(curU, curV);
        const st = useEditorStore.getState();
        if (!pixel) {
          if (st.hoveredPixel !== null || st.targetedCurveHover !== null) {
            setEditor({ hoveredPixel: null, targetedCurveHover: null });
          }
          return;
        }
        if (st.isTargetedCurveActive) {
          const xVal = channelInputValue(pixel, st.targetedCurveChannel);
          const pts = st.adjustments.curves?.[st.targetedCurveChannel] ?? getDefaultCurves()[st.targetedCurveChannel];
          const yVal = evalMonotoneCurveY(pts, xVal);
          setEditor({ hoveredPixel: pixel, targetedCurveHover: { x: xVal, y: yVal } });
        } else {
          setEditor({ hoveredPixel: pixel });
        }
      });
    },
    [sampleNormalized, setEditor],
  );

  useEffect(() => {
    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
      }
    };
  }, []);

  const exit = useCallback(() => {
    detachDragRef.current?.();
    detachDragRef.current = null;
    if (isDraggingRef.current) {
      isDraggingRef.current = false;
      setIsDragging(false);
      setEditor({ isSliderDragging: false });
    }
    setEditor({ isTargetedCurveActive: false, targetedCurveHover: null });
  }, [setEditor]);

  useEffect(() => {
    if (!isActive) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        exit();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [isActive, exit]);

  const start = useCallback(
    (e: KonvaEventObject<MouseEvent | TouchEvent>) => {
      const state = useEditorStore.getState();
      if (!state.isTargetedCurveActive) return;
      const stage = e.target.getStage();
      const pos = stage && getCanvasPointer(stage);
      if (!pos || imageRenderSize.width <= 0 || imageRenderSize.height <= 0) return;

      const u = pos.x / imageRenderSize.width;
      const v = pos.y / imageRenderSize.height;
      const pixel = sampleNormalized(u, v);
      if (!pixel) return;

      const channel = state.targetedCurveChannel;
      const rawPoints: Coord[] =
        state.adjustments.curves?.[channel]?.map((p) => ({ ...p })) ??
        getDefaultCurves()[channel].map((p) => ({ ...p }));
      rawPoints.sort((a, b) => a.x - b.x);

      const sampledX = Math.max(0, Math.min(255, channelInputValue(pixel, channel)));
      let targetIndex = -1;
      let bestDist = Infinity;

      rawPoints.forEach((pt, idx) => {
        const isEndpoint = idx === 0 || idx === rawPoints.length - 1;
        const limit = isEndpoint ? ENDPOINT_MATCH_TOLERANCE_X : POINT_MATCH_TOLERANCE_X;
        const dist = Math.abs(pt.x - sampledX);
        if (dist <= limit && dist < bestDist) {
          bestDist = dist;
          targetIndex = idx;
        }
      });

      const workingPoints = [...rawPoints];
      if (targetIndex === -1) {
        if (workingPoints.length >= MAX_CURVE_POINTS) return;
        const tooClose = workingPoints.some((p) => Math.abs(p.x - sampledX) < 1);
        const insertX = tooClose ? Math.max(1, Math.min(254, sampledX)) : sampledX;
        if (workingPoints.some((p) => Math.abs(p.x - insertX) < 1)) return;
        const insertY = evalMonotoneCurveY(workingPoints, insertX);
        workingPoints.push({ x: insertX, y: insertY });
        workingPoints.sort((a, b) => a.x - b.x);
        targetIndex = workingPoints.findIndex((p) => p.x === insertX && p.y === insertY);
        if (targetIndex === -1) return;
      }

      const evt = e.evt;
      const startClientY = 'touches' in evt ? evt.touches[0]?.clientY : evt.clientY;
      if (startClientY === undefined) return;

      detachDragRef.current?.();
      const basePoint = { ...workingPoints[targetIndex] };
      let currentY = basePoint.y;

      isDraggingRef.current = true;
      setIsDragging(true);
      setEditor({
        isSliderDragging: true,
        targetedCurveHover: { x: basePoint.x, y: basePoint.y },
      });

      const applyY = (nextY: number) => {
        const clamped = Math.max(0, Math.min(255, nextY));
        if (Math.abs(clamped - currentY) < 0.05) return;
        currentY = clamped;
        const nextPoints = workingPoints.map((pt, idx) => (idx === targetIndex ? { ...pt, y: clamped } : pt));
        setEditor({ targetedCurveHover: { x: basePoint.x, y: clamped } });
        setAdjustments(
          (prev) => ({
            ...prev,
            curveMode: 'point',
            curves: { ...prev.curves, [channel]: nextPoints },
            pointCurves: { ...(prev.pointCurves || prev.curves), [channel]: nextPoints },
          }),
          'Targeted Curve',
        );
      };

      const onMove = (ev: MouseEvent | TouchEvent) => {
        const pt = 'touches' in ev ? ev.touches[0] : ev;
        if (!pt) return;
        if (ev.cancelable) ev.preventDefault();
        const mult = hasFineAdjustmentModifier(ev) ? FINE_ADJUSTMENT_MULTIPLIER : 1;
        const dy = (startClientY - pt.clientY) * DRAG_GAIN * mult;
        applyY(basePoint.y + dy);
      };

      const onUp = () => {
        detachDragRef.current?.();
        detachDragRef.current = null;
        if (!isDraggingRef.current) return;
        isDraggingRef.current = false;
        setIsDragging(false);
        setEditor({ isSliderDragging: false });
      };

      window.addEventListener('mousemove', onMove);
      window.addEventListener('touchmove', onMove, { passive: false });
      window.addEventListener('mouseup', onUp);
      window.addEventListener('touchend', onUp);
      window.addEventListener('touchcancel', onUp);
      detachDragRef.current = () => {
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('touchmove', onMove);
        window.removeEventListener('mouseup', onUp);
        window.removeEventListener('touchend', onUp);
        window.removeEventListener('touchcancel', onUp);
      };
    },
    [getCanvasPointer, imageRenderSize.height, imageRenderSize.width, sampleNormalized, setAdjustments, setEditor],
  );

  useEffect(() => () => detachDragRef.current?.(), []);

  return { isActive, isDragging, start, exit, updateHoverAtNormalized };
}
