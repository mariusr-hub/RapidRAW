import React, { useState, useRef, useCallback } from 'react';
import { Group, Line, Circle, Arrow, Rect, Text as KonvaText } from 'react-konva';
import type { KonvaEventObject } from 'konva/lib/Node';

export interface IsophoteGuide {
  id: string;
  start: { x: number; y: number };
  end: { x: number; y: number };
  feather: number;
}

export interface ShieldRegion {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface InpaintGuideOverlayProps {
  guideMode: 'none' | 'linepen' | 'shield';
  isophoteGuides: IsophoteGuide[];
  shieldRegions: ShieldRegion[];
  zoomScale: number;
  activeGuideId?: string | null;
  onSelectGuide?: (id: string | null) => void;
  onUpdateGuide?: (guide: IsophoteGuide) => void;
  onAddGuide?: (guide: IsophoteGuide) => void;
  onRemoveGuide?: (id: string) => void;
  onUpdateShield?: (shield: ShieldRegion) => void;
  onAddShield?: (shield: ShieldRegion) => void;
  onRemoveShield?: (id: string) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  onHoverChange?: (isHovered: boolean) => void;
}

export default function InpaintGuideOverlay({
  guideMode,
  isophoteGuides,
  shieldRegions,
  zoomScale,
  activeGuideId,
  onSelectGuide,
  onUpdateGuide,
  onRemoveGuide,
  onUpdateShield,
  onRemoveShield,
  onDragStateChange,
  onHoverChange,
}: InpaintGuideOverlayProps) {
  const px = useCallback((v: number) => v / zoomScale, [zoomScale]);

  return (
    <Group listening={guideMode !== 'none'}>
      {/* 1. Exemplar Protection Shields */}
      {shieldRegions.map((shield) => {
        const isSelected = activeGuideId === shield.id;
        const strokeW = px(isSelected ? 2.5 : 1.5);
        const handleR = px(5);

        return (
          <Group
            key={shield.id}
            draggable={guideMode === 'shield'}
            x={shield.x}
            y={shield.y}
            onDragStart={(e) => {
              e.cancelBubble = true;
              onDragStateChange?.(true);
            }}
            onDragMove={(e) => {
              e.cancelBubble = true;
            }}
            onDragEnd={(e: KonvaEventObject<DragEvent>) => {
              e.cancelBubble = true;
              onDragStateChange?.(false);
              onUpdateShield?.({
                ...shield,
                x: e.target.x(),
                y: e.target.y(),
              });
            }}
            onClick={(e) => {
              e.cancelBubble = true;
              onSelectGuide?.(shield.id);
            }}
            onMouseEnter={() => onHoverChange?.(true)}
            onMouseLeave={() => onHoverChange?.(false)}
          >
            {/* Protected Area Fill & Dashed Border */}
            <Rect
              x={0}
              y={0}
              width={shield.width}
              height={shield.height}
              fill="rgba(239, 68, 68, 0.22)"
              stroke="#ef4444"
              strokeWidth={strokeW}
              dash={[px(6), px(4)]}
              cornerRadius={px(2)}
            />

            {/* Label Badge */}
            <Rect
              x={px(4)}
              y={px(4)}
              width={px(116)}
              height={px(16)}
              fill="rgba(185, 28, 28, 0.85)"
              cornerRadius={px(2)}
            />
            <KonvaText
              x={px(6)}
              y={px(6)}
              text="SHIELD: FORBIDDEN DONOR"
              fontSize={px(8.5)}
              fontFamily="monospace"
              fill="#ffffff"
              fontStyle="bold"
            />

            {/* Resize Handle (bottom-right) */}
            {isSelected && (
              <Circle
                x={shield.width}
                y={shield.height}
                radius={handleR}
                fill="#ef4444"
                stroke="#ffffff"
                strokeWidth={px(1.5)}
                draggable
                onDragStart={(e) => {
                  e.cancelBubble = true;
                  onDragStateChange?.(true);
                }}
                onDragMove={(e) => {
                  e.cancelBubble = true;
                  const newW = Math.max(px(20), e.target.x());
                  const newH = Math.max(px(20), e.target.y());
                  onUpdateShield?.({ ...shield, width: newW, height: newH });
                }}
                onDragEnd={(e) => {
                  e.cancelBubble = true;
                  onDragStateChange?.(false);
                }}
              />
            )}
          </Group>
        );
      })}

      {/* 2. Directional Isophote Guiding Lines (LinePen) */}
      {isophoteGuides.map((guide) => {
        const isSelected = activeGuideId === guide.id;
        const dx = guide.end.x - guide.start.x;
        const dy = guide.end.y - guide.start.y;
        const len = Math.hypot(dx, dy) || 1e-4;
        const nx = -dy / len;
        const ny = dx / len;
        const featherPx = guide.feather;

        // Influence corridor polygon points
        const corridorPoints = [
          guide.start.x + nx * featherPx,
          guide.start.y + ny * featherPx,
          guide.end.x + nx * featherPx,
          guide.end.y + ny * featherPx,
          guide.end.x - nx * featherPx,
          guide.end.y - ny * featherPx,
          guide.start.x - nx * featherPx,
          guide.start.y - ny * featherPx,
        ];

        return (
          <Group
            key={guide.id}
            onClick={() => onSelectGuide?.(guide.id)}
            onMouseEnter={() => onHoverChange?.(true)}
            onMouseLeave={() => onHoverChange?.(false)}
          >
            {/* Translucent Feather Corridor */}
            <Line
              points={corridorPoints}
              closed
              fill="rgba(56, 189, 248, 0.12)"
              stroke="#38bdf8"
              strokeWidth={px(1)}
              dash={[px(4), px(4)]}
              listening={false}
            />

            {/* Central Directional Arrow */}
            <Arrow
              points={[guide.start.x, guide.start.y, guide.end.x, guide.end.y]}
              stroke={isSelected ? '#38bdf8' : '#0284c7'}
              strokeWidth={px(isSelected ? 3 : 2)}
              fill={isSelected ? '#38bdf8' : '#0284c7'}
              pointerLength={px(9)}
              pointerWidth={px(7)}
              shadowColor="#000000"
              shadowBlur={px(4)}
              shadowOpacity={0.4}
            />

            {/* Start Handle (Draggable) */}
            <Circle
              x={guide.start.x}
              y={guide.start.y}
              radius={px(isSelected ? 6 : 5)}
              fill="#ffffff"
              stroke="#0284c7"
              strokeWidth={px(2)}
              draggable={guideMode === 'linepen'}
              onDragStart={(e) => {
                e.cancelBubble = true;
                onDragStateChange?.(true);
              }}
              onDragMove={(e: KonvaEventObject<DragEvent>) => {
                e.cancelBubble = true;
                onUpdateGuide?.({
                  ...guide,
                  start: { x: e.target.x(), y: e.target.y() },
                });
              }}
              onDragEnd={(e) => {
                e.cancelBubble = true;
                onDragStateChange?.(false);
              }}
            />

            {/* End Handle (Draggable) */}
            <Circle
              x={guide.end.x}
              y={guide.end.y}
              radius={px(isSelected ? 6 : 5)}
              fill="#38bdf8"
              stroke="#ffffff"
              strokeWidth={px(2)}
              draggable={guideMode === 'linepen'}
              onDragStart={(e) => {
                e.cancelBubble = true;
                onDragStateChange?.(true);
              }}
              onDragMove={(e: KonvaEventObject<DragEvent>) => {
                e.cancelBubble = true;
                onUpdateGuide?.({
                  ...guide,
                  end: { x: e.target.x(), y: e.target.y() },
                });
              }}
              onDragEnd={(e) => {
                e.cancelBubble = true;
                onDragStateChange?.(false);
              }}
            />

            {/* Tangent Orientation Indicator */}
            <KonvaText
              x={(guide.start.x + guide.end.x) / 2 + px(8)}
              y={(guide.start.y + guide.end.y) / 2 - px(10)}
              text={`Isophote ${(Math.atan2(dy, dx) * 180 / Math.PI).toFixed(0)}°`}
              fontSize={px(9)}
              fontFamily="monospace"
              fill="#38bdf8"
              fontStyle="bold"
              listening={false}
            />
          </Group>
        );
      })}
    </Group>
  );
}
