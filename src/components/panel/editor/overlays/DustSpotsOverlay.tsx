import { useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Check, Loader2, X } from 'lucide-react';
import { canvasToolState, useEditorStore } from '../../../../store/useEditorStore';
import { useEditorActions } from '../../../../hooks/useEditorActions';
import type { DetectedSpot } from '../../../../enhanced/panels/DustScratchProSection';
import type { AiPatch } from '../../../../utils/adjustments';
import { Invokes } from '../../../../types/invokes';

interface DustSpotsOverlayProps {
  crop: { x: number; y: number; width: number; height: number };
  effectiveDimensions: { width: number; height: number };
  imageRenderSize: { width: number; height: number; offsetX: number; offsetY: number; scale: number };
  routeLargeToLama?: boolean;
  zoom: number;
}

export default function DustSpotsOverlay({
  crop,
  effectiveDimensions,
  imageRenderSize,
  routeLargeToLama = true,
  zoom,
}: DustSpotsOverlayProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const spots = useEditorStore((s) => s.dustSpotCandidates);
  const setEditor = useEditorStore((s) => s.setEditor);
  const { setAdjustments } = useEditorActions();
  const applyingRef = useRef(false);

  const safeZoom = Math.max(zoom, 0.25);
  const cropW = Math.max(crop.width, 1);
  const cropH = Math.max(crop.height, 1);
  const effW = Math.max(effectiveDimensions.width, 1);
  const effH = Math.max(effectiveDimensions.height, 1);

  const handleReject = (spot: DetectedSpot, idx: number) => {
    setEditor((prev) => ({
      dustSpotCandidates: prev.dustSpotCandidates.filter((_, i) => i !== idx),
    }));
    if (spot.kind === 'dust' && selectedImage?.path) {
      invoke(Invokes.RecordDustSpotRejection, {
        cameraKey: null,
        path: selectedImage.path,
        normX: spot.norm_x,
        normY: spot.norm_y,
      }).catch((err) => console.warn('Failed to record dust spot rejection:', err));
    }
  };

  const handleCanvasClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const fx = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    const fy = Math.min(1, Math.max(0, (e.clientY - rect.top) / rect.height));
    const normX = Math.min(1, Math.max(0, (crop.x + fx * cropW) / effW));
    const normY = Math.min(1, Math.max(0, (crop.y + fy * cropH) / effH));
    const newSpot: DetectedSpot = {
      x: normX * effW,
      y: normY * effH,
      radius: 10,
      norm_x: normX,
      norm_y: normY,
      kind: 'dust',
      points: [],
      routing: 'analytical',
    };
    setEditor((prev) => ({
      dustSpotCandidates: [...prev.dustSpotCandidates, newSpot],
    }));
  };

  const handleApplyHeals = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (applyingRef.current || spots.length === 0 || !selectedImage?.path) return;
    applyingRef.current = true;
    try {
      const lamaSpots = routeLargeToLama ? spots.filter((s) => s.routing === 'lama') : [];
      const fastSpots = routeLargeToLama ? spots.filter((s) => s.routing !== 'lama') : spots;
      const newPatches: AiPatch[] = [];
      const stamp = Date.now();

      if (lamaSpots.length > 0) {
        const patchData = await invoke<any>(Invokes.HealDefectsWithLama, { spots: lamaSpots });
        newPatches.push({
          id: `dust-lama-${stamp}`,
          name: `Dust & Scratch LaMa (${lamaSpots.length})`,
          isLoading: false,
          invert: false,
          patchData,
          prompt: '',
          subMasks: [],
          visible: true,
        });
      }

      fastSpots.forEach((spot, idx) => {
        const kindLabel =
          spot.kind === 'scratch'
            ? `Scratch Heal #${idx + 1}`
            : spot.kind === 'blemish'
              ? `Blemish Heal #${idx + 1}`
              : `Dust Heal #${idx + 1}`;
        newPatches.push({
          id: `dust-heal-${stamp}-${idx}`,
          name: kindLabel,
          isLoading: false,
          invert: false,
          patchData: null,
          prompt: '',
          subMasks: [],
          visible: true,
          type: 'heal',
          kind: spot.kind || 'dust',
          x: spot.x,
          y: spot.y,
          radius: spot.radius,
          endX: spot.end_x ?? undefined,
          endY: spot.end_y ?? undefined,
          points: spot.points?.length >= 2 ? spot.points : undefined,
        });
      });

      if (newPatches.length > 0) {
        setAdjustments(
          {
            ...adjustments,
            aiPatches: [...(adjustments.aiPatches || []), ...newPatches],
          },
          'Heal Defects',
        );
        toast.success(`Applied ${newPatches.length} heal patch(es).`);
      }
      setEditor({ ...canvasToolState(), dustSpotCandidates: [] });
    } catch (err) {
      toast.error(`Defect healing failed: ${err}`);
    } finally {
      applyingRef.current = false;
    }
  };

  return (
    <div
      ref={containerRef}
      onClick={handleCanvasClick}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setEditor(canvasToolState());
      }}
      style={{
        position: 'absolute',
        left: imageRenderSize.offsetX,
        top: imageRenderSize.offsetY,
        width: imageRenderSize.width,
        height: imageRenderSize.height,
        pointerEvents: 'auto',
        cursor: 'crosshair',
        zIndex: 16,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          position: 'absolute',
          top: 10 / safeZoom,
          left: '50%',
          transform: `translateX(-50%) scale(${1 / safeZoom})`,
          transformOrigin: 'top center',
          zIndex: 20,
        }}
        className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-surface/95 backdrop-blur-md border border-white/15 shadow-xl text-xs whitespace-nowrap"
      >
        <span className="flex items-center gap-1 text-[10px] text-text-secondary">
          <span className="w-2 h-2 rounded-full bg-cyan-400" />
          <span>Heal</span>
          <span className="w-2 h-2 rounded-full bg-amber-400 ml-1" />
          <span>LaMa</span>
          <span className="w-2 h-2 rounded-full bg-rose-400 ml-1" />
          <span>Blemish</span>
        </span>
        <span className="text-[11px] font-mono text-text-primary font-medium">{`${spots.length}`}</span>
        <button
          type="button"
          disabled={spots.length === 0 || applyingRef.current}
          onClick={handleApplyHeals}
          className="flex items-center gap-1 px-2 py-0.5 rounded bg-accent text-button-text font-medium text-[11px] cursor-pointer disabled:opacity-40"
        >
          {applyingRef.current ? <Loader2 size={11} className="animate-spin" /> : <Check size={11} />}
          <span>{`Apply ${spots.length} Heals`}</span>
        </button>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setEditor(canvasToolState());
          }}
          className="px-2 py-0.5 rounded bg-bg-primary text-text-secondary hover:text-text-primary text-[11px] cursor-pointer"
        >
          Done
        </button>
      </div>

      {spots.map((spot, idx) => {
        const fx = (spot.norm_x * effW - crop.x) / cropW;
        const fy = (spot.norm_y * effH - crop.y) / cropH;
        if (fx < -0.05 || fx > 1.05 || fy < -0.05 || fy > 1.05) return null;

        const isBlemish = spot.kind === 'blemish';
        const isLama = !isBlemish && routeLargeToLama && spot.routing === 'lama';
        const ringClass = isBlemish
          ? 'border-rose-400 bg-rose-400/15 hover:bg-rose-400/25'
          : isLama
            ? 'border-amber-400 bg-amber-400/15 hover:bg-amber-400/25'
            : 'border-cyan-400 bg-cyan-400/15 hover:bg-cyan-400/25';

        const diameter = Math.max(14 / safeZoom, spot.radius * 2 * imageRenderSize.scale);
        const btnScale = 1 / safeZoom;

        return (
          <div
            key={`${spot.kind}-${Math.round(spot.x)}-${Math.round(spot.y)}-${idx}`}
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              handleReject(spot, idx);
            }}
            onWheel={(e) => {
              e.stopPropagation();
              e.preventDefault();
              const delta = e.deltaY < 0 ? 1.5 : -1.5;
              setEditor((prev) => ({
                dustSpotCandidates: prev.dustSpotCandidates.map((s, i) =>
                  i === idx ? { ...s, radius: Math.min(60, Math.max(2, s.radius + delta)) } : s,
                ),
              }));
            }}
            title={`Click × or right-click to reject • Scroll wheel to resize (${spot.radius.toFixed(1)}px)`}
            style={{
              position: 'absolute',
              left: `${fx * 100}%`,
              top: `${fy * 100}%`,
              width: diameter,
              height: diameter,
              transform: 'translate(-50%, -50%)',
              borderWidth: Math.max(1, 1.5 * btnScale),
            }}
            className={`group rounded-full border transition-colors ${ringClass}`}
          >
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                handleReject(spot, idx);
              }}
              style={{
                transform: `translate(35%, -35%) scale(${btnScale})`,
                transformOrigin: 'top right',
              }}
              className="opacity-0 group-hover:opacity-100 absolute top-0 right-0 w-4 h-4 rounded-full bg-black/85 text-white flex items-center justify-center border border-white/30 cursor-pointer transition-opacity"
            >
              <X size={9} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
