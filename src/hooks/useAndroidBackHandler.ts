import { useEffect } from 'react';
import { closeMergeModal, useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';

export function useAndroidBackHandler() {
  useEffect(() => {
    const osPlatform = useSettingsStore.getState().osPlatform;
    if (osPlatform !== 'android') return;

    (window as any).__handleAndroidBack = () => {
      const ui = useUIStore.getState();

      if (ui.confirmModalState.isOpen) {
        ui.setUI((state: any) => ({ confirmModalState: { ...state.confirmModalState, isOpen: false } }));
        return;
      }
      if (ui.isCreateFolderModalOpen) {
        ui.setUI({ isCreateFolderModalOpen: false });
        return;
      }
      if (ui.isRenameFolderModalOpen) {
        ui.setUI({ isRenameFolderModalOpen: false });
        return;
      }
      if (ui.isRenameFileModalOpen) {
        ui.setUI({ isRenameFileModalOpen: false });
        return;
      }
      if (ui.isImportModalOpen) {
        ui.setUI({ isImportModalOpen: false });
        return;
      }
      if (ui.isCopyPasteSettingsModalOpen) {
        ui.setUI({ isCopyPasteSettingsModalOpen: false });
        return;
      }
      if (ui.isCreateAlbumModalOpen) {
        ui.setUI({ isCreateAlbumModalOpen: false });
        return;
      }
      if (ui.isCreateAlbumGroupModalOpen) {
        ui.setUI({ isCreateAlbumGroupModalOpen: false });
        return;
      }
      if (ui.isRenameAlbumModalOpen) {
        ui.setUI({ isRenameAlbumModalOpen: false });
        return;
      }
      if (ui.mergeModalState.isOpen) {
        closeMergeModal();
        return;
      }
      if (ui.cullingModalState.isOpen) {
        ui.setUI({
          cullingModalState: { isOpen: false, progress: null, suggestions: null, error: null, pathsToCull: [] },
        });
        return;
      }
      if (ui.collageModalState.isOpen) {
        ui.setUI({ collageModalState: { isOpen: false, sourceImages: [] } });
        return;
      }
      if (ui.drizzleModalState.isOpen) {
        ui.setUI((s) => ({ drizzleModalState: { ...s.drizzleModalState, isOpen: false } }));
        return;
      }
      if (ui.negativeModalState.isOpen) {
        ui.setUI((s) => ({ negativeModalState: { ...s.negativeModalState, isOpen: false } }));
        return;
      }
      if (ui.denoiseModalState.isOpen) {
        ui.setUI((s) => ({ denoiseModalState: { ...s.denoiseModalState, isOpen: false } }));
        return;
      }
      if (ui.batchPolishModalState.isOpen) {
        ui.setUI({ batchPolishModalState: { isOpen: false, selectedPaths: [] } });
        return;
      }
      if (ui.colorMatcherModalState.isOpen) {
        ui.setUI({ colorMatcherModalState: { isOpen: false } });
        return;
      }
      if (ui.heroCuratorModalState.isOpen) {
        ui.setUI({ heroCuratorModalState: { isOpen: false, selectedPaths: [] } });
        return;
      }
      if (ui.clientDeliveryModalState.isOpen) {
        ui.setUI({ clientDeliveryModalState: { isOpen: false, selectedPaths: [] } });
        return;
      }
      if (ui.bokehModalState.isOpen) {
        ui.setUI({ bokehModalState: { isOpen: false } });
        return;
      }
      if (ui.advancedExportModalState.isOpen) {
        ui.setUI({ advancedExportModalState: { isOpen: false, selectedPaths: [] } });
        return;
      }
      if (ui.speedCullerModalState.isOpen) {
        ui.setUI({ speedCullerModalState: { isOpen: false, selectedPaths: [], initialIndex: 0 } });
        return;
      }
      if (ui.isLensCorrectionModalOpen) {
        ui.setUI({ isLensCorrectionModalOpen: false });
        return;
      }
      if (ui.isTransformModalOpen) {
        ui.setUI({ isTransformModalOpen: false });
        return;
      }

      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }),
      );
    };

    return () => {
      delete (window as any).__handleAndroidBack;
    };
  }, []);
}
