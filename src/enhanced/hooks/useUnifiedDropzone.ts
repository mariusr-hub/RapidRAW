import { useCallback, useEffect, useRef, useState } from 'react';
import { useDroppable } from '@dnd-kit/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';

interface UseUnifiedDropzoneOptions {
  id: string;
  onDropPaths: (paths: string[]) => void;
}

function isPointInsideElement(el: HTMLElement | null, x: number, y: number): boolean {
  if (!el) return false;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  const dpr = window.devicePixelRatio || 1;
  const lx = x / dpr;
  const ly = y / dpr;
  return (
    (lx >= rect.left && lx <= rect.right && ly >= rect.top && ly <= rect.bottom) ||
    (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)
  );
}

export function useUnifiedDropzone({ id, onDropPaths }: UseUnifiedDropzoneOptions) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const onDropRef = useRef(onDropPaths);
  onDropRef.current = onDropPaths;

  const [isNativeOver, setIsNativeOver] = useState(false);

  const { setNodeRef, isOver: isDndOver } = useDroppable({
    id,
    data: {
      type: 'enhanced-dropzone',
      onDropPaths: (paths: string[]) => onDropRef.current(paths),
    },
  });

  const combinedRef = useCallback(
    (node: HTMLDivElement | null) => {
      containerRef.current = node;
      setNodeRef(node);
    },
    [setNodeRef],
  );

  useEffect(() => {
    let active = true;
    let unlistenFn: (() => void) | null = null;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!active) return;
        const payload = event.payload as any;
        if (payload?.type === 'enter' || payload?.type === 'over') {
          const pos = payload.position;
          setIsNativeOver(pos ? isPointInsideElement(containerRef.current, pos.x, pos.y) : false);
        } else if (payload?.type === 'drop') {
          const pos = payload.position;
          const inside = pos ? isPointInsideElement(containerRef.current, pos.x, pos.y) : false;
          setIsNativeOver(false);
          if (inside && Array.isArray(payload.paths) && payload.paths.length > 0) {
            onDropRef.current(payload.paths);
          }
        } else if (payload?.type === 'leave') {
          setIsNativeOver(false);
        }
      })
      .then((unlisten) => {
        if (active) {
          unlistenFn = unlisten;
        } else {
          unlisten();
        }
      })
      .catch(() => {
        // Ignore on environments without Tauri webview listener support
      });

    return () => {
      active = false;
      if (unlistenFn) unlistenFn();
    };
  }, []);

  return {
    ref: combinedRef,
    isDragActive: isDndOver || isNativeOver,
  };
}
