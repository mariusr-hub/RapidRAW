import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Loader2, Search, Sparkles, X } from 'lucide-react';
import CollapsibleSection from '../../components/ui/CollapsibleSection';
import Button from '../../components/ui/Button';
import Switch from '../../components/ui/Switch';
import { useEditorStore } from '../../store/useEditorStore';
import { useEditorActions } from '../../hooks/useEditorActions';
import { AiPatch } from '../../utils/adjustments';
import { Invokes } from '../../types/invokes';

export interface DetectedSpot {
  x: number;
  y: number;
  radius: number;
  norm_x: number;
  norm_y: number;
  kind: string;
  end_x?: number | null;
  end_y?: number | null;
  norm_end_x?: number | null;
  norm_end_y?: number | null;
  points: [number, number][];
  routing: string;
}

export interface AutoDefectRepairResult {
  spots: DetectedSpot[];
  total_detected: number;
  message: string;
}

export interface SensorDustMapSpot {
  id: string;
  sensorX: number;
  sensorY: number;
  medianRadius: number;
  hitCount: number;
  frameCount: number;
  confidence: number;
  cameraKey: string;
}

export interface ShootDustMapReport {
  cameraKey: string;
  totalFrames: number;
  spots: SensorDustMapSpot[];
  message: string;
}

export default function DustScratchProSection() {
  const selectedImage = useEditorStore((state) => state.selectedImage);
  const adjustments = useEditorStore((state) => state.adjustments);
  const { setAdjustments } = useEditorActions();

  const [isOpen, setIsOpen] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [isHealing, setIsHealing] = useState(false);
  const [routeLargeToLama, setRouteLargeToLama] = useState(true);
  const [spots, setSpots] = useState<DetectedSpot[]>([]);

  useEffect(() => {
    setSpots([]);
  }, [selectedImage?.path]);

  const handleScanDefects = async () => {
    if (!selectedImage || isScanning || isHealing) return;
    setIsScanning(true);
    try {
      const result = await invoke<AutoDefectRepairResult>(Invokes.AutoRemoveDustAndBlemishes);
      setSpots(result.spots || []);
      if ((result.spots || []).length === 0) {
        toast.info(result.message || 'No dust spots or scratches detected.');
      } else {
        setIsOpen(true);
        toast.success(result.message || `Detected ${result.total_detected} defect(s).`);
      }
    } catch (err) {
      toast.error(`Defect scan failed: ${err}`);
    } finally {
      setIsScanning(false);
    }
  };

  const handleRejectSpot = async (spot: DetectedSpot, index: number) => {
    if (!selectedImage?.path || isHealing) return;
    try {
      await invoke(Invokes.RecordDustSpotRejection, {
        cameraKey: null,
        path: selectedImage.path,
        normX: spot.norm_x,
        normY: spot.norm_y,
      });
      setSpots((prev) => prev.filter((_, i) => i !== index));
    } catch (err) {
      toast.error(`Failed to record spot rejection: ${err}`);
    }
  };

  const handleHealAll = async () => {
    if (!selectedImage || spots.length === 0 || isHealing || isScanning) return;
    setIsHealing(true);
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
        newPatches.push({
          id: `dust-heal-${stamp}-${idx}`,
          name: spot.kind === 'scratch' ? `Scratch Heal #${idx + 1}` : `Dust Heal #${idx + 1}`,
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
          'Dust & Scratch Pro'
        );
        setSpots([]);
        toast.success(`Applied ${newPatches.length} defect heal layer(s).`);
      }
    } catch (err) {
      toast.error(`Defect healing failed: ${err}`);
    } finally {
      setIsHealing(false);
    }
  };

  const lamaCount = spots.filter((s) => s.routing === 'lama').length;

  return (
    <div className="mt-3 pt-2 border-t border-white/10" onClick={(e) => e.stopPropagation()}>
      <CollapsibleSection
        title="Dust & Scratch Pro"
        isOpen={isOpen}
        onToggle={() => setIsOpen((v) => !v)}
        isContentVisible={true}
        canToggleVisibility={false}
      >
        <div className="flex flex-col gap-2.5 pt-1 pb-2">
          <Switch
            label="Auto-route large/textured defects to LaMa AI"
            checked={routeLargeToLama}
            onChange={(checked: boolean) => setRouteLargeToLama(checked)}
          />

          <div className="flex items-center gap-2">
            <Button
              size="sm"
              className="flex-1 text-xs flex items-center justify-center gap-1.5"
              disabled={!selectedImage || isScanning || isHealing}
              onClick={handleScanDefects}
            >
              {isScanning ? <Loader2 size={12} className="animate-spin" /> : <Search size={12} />}
              <span>{isScanning ? 'Scanning...' : 'Scan Defects'}</span>
            </Button>

            {spots.length > 0 && (
              <Button
                size="sm"
                className="flex-1 text-xs bg-accent/20 hover:bg-accent/30 text-text-primary border border-accent/40 flex items-center justify-center gap-1.5"
                disabled={!selectedImage || isHealing || isScanning}
                onClick={handleHealAll}
              >
                {isHealing ? (
                  <Loader2 size={12} className="animate-spin text-accent" />
                ) : (
                  <Sparkles size={12} className="text-accent" />
                )}
                <span>{`Heal All (${spots.length})`}</span>
              </Button>
            )}
          </div>

          {spots.length > 0 && (
            <div className="flex flex-col gap-1 max-h-44 overflow-y-auto pr-1 custom-scrollbar">
              <div className="text-[10px] text-text-secondary flex items-center justify-between px-1">
                <span>{`${spots.length} candidate(s) detected`}</span>
                {routeLargeToLama && lamaCount > 0 && (
                  <span className="text-accent">{`${lamaCount} routed to LaMa`}</span>
                )}
              </div>

              {spots.map((spot, idx) => {
                const useLama = routeLargeToLama && spot.routing === 'lama';
                return (
                  <div
                    key={`${spot.kind}-${Math.round(spot.x)}-${Math.round(spot.y)}-${idx}`}
                    className="flex items-center justify-between gap-2 px-2 py-1 rounded bg-surface/60 border border-white/5 text-[11px]"
                  >
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span className="text-text-secondary font-mono text-[10px]">#{idx + 1}</span>
                      <span className="capitalize text-text-primary truncate">
                        {spot.kind === 'scratch' ? 'Scratch' : 'Dust'}
                      </span>
                      <span className="text-text-secondary text-[10px]">
                        {`r=${spot.radius.toFixed(1)}`}
                      </span>
                      <span
                        className={
                          useLama
                            ? 'px-1 py-0.5 rounded bg-accent/20 text-accent text-[9px] font-medium'
                            : 'px-1 py-0.5 rounded bg-white/10 text-text-secondary text-[9px]'
                        }
                      >
                        {useLama ? 'LaMa' : 'Fast'}
                      </span>
                    </div>

                    <button
                      type="button"
                      className="p-0.5 rounded hover:bg-white/10 text-text-secondary hover:text-text-primary transition-colors"
                      title="Reject false positive (suppresses future hits here)"
                      onClick={() => handleRejectSpot(spot, idx)}
                    >
                      <X size={12} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </CollapsibleSection>
    </div>
  );
}
