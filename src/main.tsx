import React from 'react';
import { createRoot } from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App';
import ViewerWindow from './window/ViewerWindow';
import { installFrontendLogBridge } from './utils/frontendLogBridge';
import { initDisplayScale } from './utils/displayScale';
import { useUIStore } from './store/useUIStore';
import { useEditorStore } from './store/useEditorStore';
import { useLibraryStore } from './store/useLibraryStore';
import { useSettingsStore } from './store/useSettingsStore';
import { useProcessStore } from './store/useProcessStore';
import './styles.css';

installFrontendLogBridge();
initDisplayScale();

if ((window as any).__DOCS_CAPTURE__) {
  (window as any).__DOCS_STORES__ = {
    useUIStore,
    useEditorStore,
    useLibraryStore,
    useSettingsStore,
    useProcessStore,
  };
}

const isViewerWindow = getCurrentWindow().label === 'viewer';

const root = createRoot(document.getElementById('root')!);
root.render(<React.StrictMode>{isViewerWindow ? <ViewerWindow /> : <App />}</React.StrictMode>);
