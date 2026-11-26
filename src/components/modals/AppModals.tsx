import { Component, type ErrorInfo, type ReactNode, Suspense, lazy } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useTranslation } from 'react-i18next';
import { closeMergeModal, useUIStore } from '../../store/useUIStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { selectAiModelDownloadStatus, useProcessStore } from '../../store/useProcessStore';
import { useEditorStore } from '../../store/useEditorStore';
import CopyPasteSettingsModal from './CopyPasteSettingsModal';
import MergeModal from './MergeModal';
import CreateFolderModal from './CreateFolderModal';
import RenameFolderModal from './RenameFolderModal';
import RenameFileModal from './RenameFileModal';
import ConfirmModal from './ConfirmModal';
import ImportSettingsModal from './ImportSettingsModal';
import CullingModal from './CullingModal';
import CollageModal from './CollageModal';
import type { DenoiseMethod } from '../../enhanced/modals/DenoiseModal';
import { AppSettings } from '../../types/settings';
import { ImageFlag, AlbumItem, AlbumGroup, MergeSettings } from '../../types/library';
import { CopyPasteSettings } from '../../utils/adjustments';

const NegativeConversionModal = lazy(() => import('../../enhanced/modals/NegativeConversionModal'));
const DenoiseModal = lazy(() => import('../../enhanced/modals/DenoiseModal'));
const BatchPolishModal = lazy(() => import('../../enhanced/modals/BatchPolishModal'));
const ColorMatcherModal = lazy(() => import('../../enhanced/modals/ColorMatcherModal'));
const HeroCuratorModal = lazy(() => import('../../enhanced/modals/HeroCuratorModal'));
const ClientDeliveryModal = lazy(() => import('../../enhanced/modals/ClientDeliveryModal'));
const BokehModal = lazy(() => import('../../enhanced/modals/BokehModal'));
const AdvancedExportModal = lazy(() => import('../../enhanced/modals/AdvancedExportModal'));
const SpeedCullerModal = lazy(() => import('../../enhanced/modals/SpeedCullerModal'));
const DrizzleModal = lazy(() => import('../../enhanced/modals/DrizzleModal'));
const LensCorrectionModal = lazy(() => import('../../enhanced/modals/LensCorrectionModal'));
const TransformModal = lazy(() => import('../../enhanced/modals/TransformModal'));

export interface AppModalsProps {
  handleImageSelect: (path: string) => void;
  handleMergeSettingsChange: (settings: MergeSettings) => void;
  handleSaveMerge: () => Promise<string>;
  handleStartMerge: () => void;
  refreshImageList: () => Promise<void>;
  handleApplyDenoise: (
    intensity: number,
    method: DenoiseMethod,
    healDust?: boolean,
    visualizeDefects?: boolean,
    protectStars?: boolean,
    preserveDetails?: number,
    chromaIntensity?: number,
    shadowBoost?: number,
    deband?: boolean,
    filmGrain?: number,
  ) => Promise<void> | void;
  handleBatchDenoise: (
    intensity: number,
    method: DenoiseMethod,
    paths: string[],
    healDust?: boolean,
    protectStars?: boolean,
    preserveDetails?: number,
    chromaIntensity?: number,
    shadowBoost?: number,
    deband?: boolean,
    filmGrain?: number,
  ) => Promise<string[]>;
  handleSaveDenoisedImage: () => Promise<string>;
  handleCreateFolder: (folderName: string) => Promise<void>;
  handleRenameFolder: (newName: string) => Promise<void>;
  handleSaveRename: (nameTemplate: string) => Promise<void>;
  handleStartImport: (settings: any) => Promise<void>;
  handleSetFlag: (flag: ImageFlag | null, paths?: string[]) => void;
  handleRate: (rating: number, paths?: string[]) => void;
  executeDelete: (paths: string[], options: any) => Promise<void>;
  handleSaveCollage: (base64Data: string, firstPath: string) => Promise<string>;
  handleCreateAlbumItem: (name: string, type: 'album' | 'group') => Promise<void>;
  handleRenameAlbumItem: (newName: string) => Promise<void>;
}

class ModalErrorBoundary extends Component<
  { children: ReactNode; onReset: () => void },
  { hasError: boolean; errorMessage: string }
> {
  constructor(props: { children: ReactNode; onReset: () => void }) {
    super(props);
    this.state = { hasError: false, errorMessage: '' };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, errorMessage: error?.message || String(error) };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Modal render error caught by ModalErrorBoundary:', error, info);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="fixed inset-0 z-100 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4">
          <div className="bg-surface border border-border-color rounded-xl shadow-2xl p-6 max-w-md w-full text-text-primary space-y-4">
            <h3 className="text-base font-semibold text-red-400">Modal Error</h3>
            <p className="text-xs text-text-secondary bg-bg-primary p-3 rounded-lg break-words">
              {this.state.errorMessage || 'An unexpected rendering error occurred.'}
            </p>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => {
                  this.props.onReset();
                  this.setState({ hasError: false, errorMessage: '' });
                }}
                className="px-4 py-2 rounded-md bg-accent text-button-text text-sm font-medium cursor-pointer"
              >
                Reset &amp; Close Modal
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export default function AppModals(props: AppModalsProps) {
  const { t } = useTranslation();
  const { appSettings, handleSettingsChange } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      handleSettingsChange: state.handleSettingsChange,
    })),
  );

  const {
    isCreateFolderModalOpen,
    isRenameFolderModalOpen,
    isRenameFileModalOpen,
    isImportModalOpen,
    isCopyPasteSettingsModalOpen,
    folderActionTarget,
    renameTargetPaths,
    importSourcePaths,
    isCreateAlbumModalOpen,
    isCreateAlbumGroupModalOpen,
    isRenameAlbumModalOpen,
    albumActionTarget,
    confirmModalState,
    mergeModalState,
    cullingModalState,
    collageModalState,
    negativeModalState,
    denoiseModalState,
    batchPolishModalState,
    colorMatcherModalState,
    heroCuratorModalState,
    clientDeliveryModalState,
    bokehModalState,
    advancedExportModalState,
    isLensCorrectionModalOpen,
    isTransformModalOpen,
    setUI,
  } = useUIStore(
    useShallow((state) => ({
      isCreateFolderModalOpen: state.isCreateFolderModalOpen,
      isRenameFolderModalOpen: state.isRenameFolderModalOpen,
      isRenameFileModalOpen: state.isRenameFileModalOpen,
      isImportModalOpen: state.isImportModalOpen,
      isCopyPasteSettingsModalOpen: state.isCopyPasteSettingsModalOpen,
      folderActionTarget: state.folderActionTarget,
      renameTargetPaths: state.renameTargetPaths,
      importSourcePaths: state.importSourcePaths,
      isCreateAlbumModalOpen: state.isCreateAlbumModalOpen,
      isCreateAlbumGroupModalOpen: state.isCreateAlbumGroupModalOpen,
      isRenameAlbumModalOpen: state.isRenameAlbumModalOpen,
      albumActionTarget: state.albumActionTarget,
      confirmModalState: state.confirmModalState,
      mergeModalState: state.mergeModalState,
      cullingModalState: state.cullingModalState,
      collageModalState: state.collageModalState,
      negativeModalState: state.negativeModalState,
      denoiseModalState: state.denoiseModalState,
      batchPolishModalState: state.batchPolishModalState,
      colorMatcherModalState: state.colorMatcherModalState,
      heroCuratorModalState: state.heroCuratorModalState,
      clientDeliveryModalState: state.clientDeliveryModalState,
      bokehModalState: state.bokehModalState,
      advancedExportModalState: state.advancedExportModalState,
      isLensCorrectionModalOpen: state.isLensCorrectionModalOpen,
      isTransformModalOpen: state.isTransformModalOpen,
      setUI: state.setUI,
    })),
  );

  const { selectedImage, adjustments, finalPreviewUrl, setEditor, pushHistory } = useEditorStore(
    useShallow((state) => ({
      selectedImage: state.selectedImage,
      adjustments: state.adjustments,
      finalPreviewUrl: state.finalPreviewUrl,
      setEditor: state.setEditor,
      pushHistory: state.pushHistory,
    })),
  );

  const mergeSourcePaths = mergeModalState.sourcePaths;
  const mergeLoadingPath =
    mergeSourcePaths.length > 0 ? mergeSourcePaths[Math.floor(mergeSourcePaths.length / 2)] : null;
  const denoiseLoadingPath =
    denoiseModalState.targetPaths.length > 0 ? denoiseModalState.targetPaths[0] : null;
  const { mergeThumbnail, denoiseThumbnail, aiModelDownloadStatus } = useProcessStore(
    useShallow((state) => ({
      mergeThumbnail: mergeLoadingPath ? state.thumbnails[mergeLoadingPath] : undefined,
      denoiseThumbnail: denoiseLoadingPath ? state.thumbnails[denoiseLoadingPath] : undefined,
      aiModelDownloadStatus: selectAiModelDownloadStatus(state),
    })),
  );

  const closeConfirmModal = () => {
    setUI((state) => ({ confirmModalState: { ...state.confirmModalState, isOpen: false } }));
  };

  const currentAlbumData = (() => {
    if (!albumActionTarget) return null;
    const { albumTree } = useLibraryStore.getState();
    const findNode = (nodes: AlbumItem[]): AlbumItem | null => {
      for (const n of nodes) {
        if (n.id === albumActionTarget) return n;
        if (n.type === 'group') {
          const res = findNode((n as AlbumGroup).children);
          if (res) return res;
        }
      }
      return null;
    };
    return findNode(albumTree);
  })();

  const currentAlbumName = currentAlbumData?.name || '';
  const isAlbumGroup = currentAlbumData?.type === 'group';

  return (
    <ModalErrorBoundary
      onReset={() => {
        closeMergeModal();
        setUI((state) => ({
          denoiseModalState: { ...state.denoiseModalState, isOpen: false, isProcessing: false },
          drizzleModalState: { ...state.drizzleModalState, isOpen: false, isProcessing: false },
          cullingModalState: { ...state.cullingModalState, isOpen: false, progress: null },
        }));
      }}
    >
      <CopyPasteSettingsModal
        isOpen={isCopyPasteSettingsModalOpen}
        onClose={() => setUI({ isCopyPasteSettingsModalOpen: false })}
        settings={appSettings?.copyPasteSettings as CopyPasteSettings}
        onSave={(newSettings) =>
          handleSettingsChange({ ...appSettings, copyPasteSettings: newSettings } as AppSettings)
        }
      />
      <MergeModal
        loadingImageUrl={mergeThumbnail || null}
        onClose={closeMergeModal}
        onOpenFile={(path: string) => props.handleImageSelect(path)}
        onSave={props.handleSaveMerge}
        onSettingsChange={props.handleMergeSettingsChange}
        onStart={props.handleStartMerge}
        state={mergeModalState}
      />
      <CreateFolderModal
        isOpen={isCreateFolderModalOpen}
        onClose={() => setUI({ isCreateFolderModalOpen: false })}
        onSave={props.handleCreateFolder}
      />
      <RenameFolderModal
        currentName={folderActionTarget ? folderActionTarget.split(/[\\/]/).pop() || '' : ''}
        isOpen={isRenameFolderModalOpen}
        onClose={() => setUI({ isRenameFolderModalOpen: false })}
        onSave={props.handleRenameFolder}
      />
      <CreateFolderModal
        isOpen={isCreateAlbumModalOpen}
        onClose={() => setUI({ isCreateAlbumModalOpen: false })}
        onSave={(name) => props.handleCreateAlbumItem(name, 'album')}
        title={t('contextMenus.albums.newAlbum')}
        placeholder={t('modals.createAlbum.placeholder')}
        buttonText={t('modals.createFolder.create')}
      />
      <CreateFolderModal
        isOpen={isCreateAlbumGroupModalOpen}
        onClose={() => setUI({ isCreateAlbumGroupModalOpen: false })}
        onSave={(name) => props.handleCreateAlbumItem(name, 'group')}
        title={t('contextMenus.albums.newGroup')}
        placeholder={t('modals.createGroup.placeholder')}
        buttonText={t('modals.createFolder.create')}
      />
      <RenameFolderModal
        currentName={currentAlbumName}
        isOpen={isRenameAlbumModalOpen}
        onClose={() => setUI({ isRenameAlbumModalOpen: false })}
        onSave={props.handleRenameAlbumItem}
        title={isAlbumGroup ? t('contextMenus.albums.renameGroup') : t('contextMenus.albums.renameAlbum')}
        placeholder={isAlbumGroup ? t('modals.renameGroup.placeholder') : t('modals.renameAlbum.placeholder')}
      />
      <RenameFileModal
        filesToRename={renameTargetPaths}
        isOpen={isRenameFileModalOpen}
        onClose={() => setUI({ isRenameFileModalOpen: false })}
        onSave={props.handleSaveRename}
      />
      <ConfirmModal {...confirmModalState} onClose={closeConfirmModal} />
      <ImportSettingsModal
        fileCount={importSourcePaths.length}
        isOpen={isImportModalOpen}
        onClose={() => setUI({ isImportModalOpen: false })}
        onSave={props.handleStartImport}
      />
      <CullingModal
        isOpen={cullingModalState.isOpen}
        onClose={() =>
          setUI({
            cullingModalState: { isOpen: false, progress: null, suggestions: null, error: null, pathsToCull: [] },
          })
        }
        progress={cullingModalState.progress}
        suggestions={cullingModalState.suggestions}
        error={cullingModalState.error}
        imagePaths={cullingModalState.pathsToCull}
        onApply={(action, paths) => {
          if (action === 'reject') {
            props.handleSetFlag(ImageFlag.Reject, paths);
          } else if (action === 'rate_zero') {
            props.handleRate(1, paths);
          } else if (action === 'delete') {
            props.executeDelete(paths, { includeAssociated: false });
          }
          setUI({
            cullingModalState: { isOpen: false, progress: null, suggestions: null, error: null, pathsToCull: [] },
          });
        }}
        onError={(err) => {
          setUI((state) => ({ cullingModalState: { ...state.cullingModalState, error: err, progress: null } }));
        }}
      />
      <CollageModal
        isOpen={collageModalState.isOpen}
        onClose={() => setUI({ collageModalState: { isOpen: false, sourceImages: [] } })}
        onSave={props.handleSaveCollage}
        sourceImages={collageModalState.sourceImages}
      />
      <Suspense fallback={null}>
        <NegativeConversionModal
          isOpen={negativeModalState.isOpen}
          onClose={() => setUI((state) => ({ negativeModalState: { ...state.negativeModalState, isOpen: false } }))}
          targetPaths={negativeModalState.targetPaths}
          onSave={(savedPaths) => {
            props.refreshImageList().then(() => {
              if (
                selectedImage &&
                negativeModalState.targetPaths.includes(selectedImage.path) &&
                savedPaths.length > 0
              ) {
                props.handleImageSelect(savedPaths[0]);
              }
            });
          }}
        />
        <DenoiseModal
          isOpen={denoiseModalState.isOpen}
          onClose={() => setUI((state) => ({ denoiseModalState: { ...state.denoiseModalState, isOpen: false } }))}
          onDenoise={props.handleApplyDenoise}
          onBatchDenoise={props.handleBatchDenoise}
          onSave={props.handleSaveDenoisedImage}
          onOpenFile={props.handleImageSelect}
          previewBase64={denoiseModalState.previewBase64}
          originalBase64={denoiseModalState.originalBase64 || null}
          isProcessing={denoiseModalState.isProcessing}
          error={denoiseModalState.error}
          progressMessage={denoiseModalState.progressMessage}
          aiModelDownloadStatus={aiModelDownloadStatus}
          isRaw={denoiseModalState.isRaw}
          targetPaths={denoiseModalState.targetPaths}
          loadingImageUrl={
            denoiseLoadingPath
              ? denoiseThumbnail || (selectedImage?.path === denoiseLoadingPath ? finalPreviewUrl : null)
              : null
          }
        />
        <BatchPolishModal
          isOpen={batchPolishModalState.isOpen}
          onClose={() => setUI({ batchPolishModalState: { isOpen: false, selectedPaths: [] } })}
          selectedPaths={batchPolishModalState.selectedPaths}
          onComplete={props.refreshImageList}
        />
        <ColorMatcherModal
          isOpen={colorMatcherModalState.isOpen}
          onClose={() => setUI({ colorMatcherModalState: { isOpen: false, heroPath: null, targetPaths: [] } })}
          heroPath={colorMatcherModalState.heroPath}
          targetPaths={colorMatcherModalState.targetPaths}
          initialMode={colorMatcherModalState.initialMode}
        />
        <HeroCuratorModal
          isOpen={heroCuratorModalState.isOpen}
          onClose={() => setUI({ heroCuratorModalState: { isOpen: false, selectedPaths: [] } })}
          selectedPaths={heroCuratorModalState.selectedPaths}
          onSelectWinner={(path) => {
            props.handleImageSelect(path);
            props.refreshImageList();
          }}
        />
        <ClientDeliveryModal
          isOpen={clientDeliveryModalState.isOpen}
          onClose={() => setUI({ clientDeliveryModalState: { isOpen: false, selectedPaths: [] } })}
          selectedPaths={clientDeliveryModalState.selectedPaths}
        />
        <BokehModal
          isOpen={bokehModalState.isOpen}
          onClose={() => setUI({ bokehModalState: { isOpen: false } })}
        />
        <AdvancedExportModal
          isOpen={advancedExportModalState.isOpen}
          onClose={() => setUI({ advancedExportModalState: { isOpen: false, selectedPaths: [] } })}
          selectedPaths={advancedExportModalState.selectedPaths}
        />
        <SpeedCullerModal />
        <DrizzleModal onOpenFile={props.handleImageSelect} />
        <LensCorrectionModal
          isOpen={isLensCorrectionModalOpen}
          onClose={() => setUI({ isLensCorrectionModalOpen: false })}
          currentAdjustments={adjustments}
          selectedImage={selectedImage}
          onApply={(newParams) => {
            const next = { ...adjustments, ...newParams };
            setEditor({ adjustments: next });
            pushHistory(next, 'Lens Correction');
          }}
        />
        <TransformModal
          isOpen={isTransformModalOpen}
          onClose={() => setUI({ isTransformModalOpen: false })}
          currentAdjustments={adjustments}
          onApply={(newParams) => {
            const next = {
              ...adjustments,
              transformDistortion: newParams.distortion,
              transformVertical: newParams.vertical,
              transformHorizontal: newParams.horizontal,
              transformRotate: newParams.rotate,
              transformAspect: newParams.aspect,
              transformScale: newParams.scale,
              transformXOffset: newParams.x_offset,
              transformYOffset: newParams.y_offset,
            };
            setEditor({ adjustments: next });
            pushHistory(next, 'Transform Geometry');
          }}
        />
      </Suspense>
    </ModalErrorBoundary>
  );
}
