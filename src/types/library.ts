export enum RawStatus {
  All = 'all',
  NonRawOnly = 'nonRawOnly',
  RawOnly = 'rawOnly',
}

export enum SortDirection {
  Ascending = 'asc',
  Descending = 'desc',
}

export const EditedStatus = {
  All: 'all',
  EditedOnly: 'editedOnly',
  UneditedOnly: 'uneditedOnly',
} as const;

export type EditedStatus = (typeof EditedStatus)[keyof typeof EditedStatus];

export const ImageFlag = {
  Pick: 'pick',
  Reject: 'reject',
} as const;

export type ImageFlag = (typeof ImageFlag)[keyof typeof ImageFlag];

export const FlagStatus = {
  All: 'all',
  Picked: 'picked',
  ExcludeRejected: 'excludeRejected',
  Rejected: 'rejected',
} as const;

export type FlagStatus = (typeof FlagStatus)[keyof typeof FlagStatus];

export type RatingOperator = 'gte' | 'eq' | 'lte';

export const RATING_OPERATORS = {
  lte: {
    symbol: '≤',
    next: 'eq',
    labelKey: 'library.header.viewOptions.ratingOperator.lte',
    suffixKey: 'library.filters.rating.andDownSuffix',
  },
  eq: {
    symbol: '=',
    next: 'gte',
    labelKey: 'library.header.viewOptions.ratingOperator.eq',
    suffixKey: 'library.filters.rating.onlySuffix',
  },
  gte: {
    symbol: '≥',
    next: 'lte',
    labelKey: 'library.header.viewOptions.ratingOperator.gte',
    suffixKey: 'library.filters.rating.andUpSuffix',
  },
} as const;

export interface FilterCriteria {
  colors: Array<string>;
  rating: number;
  ratingOperator?: RatingOperator;
  rawStatus: RawStatus;
  editedStatus?: EditedStatus;
  flagStatus?: FlagStatus;
}

export interface Folder {
  children: any;
  id?: string | undefined;
  name?: string | undefined;
  imageCount?: number;
}

export interface ImageFile {
  is_edited: boolean;
  modified: number;
  path: string;
  rating: number;
  flag: ImageFlag | null;
  tags: Array<string> | null;
  exif: { [key: string]: string } | null;
  is_virtual_copy: boolean;
  is_cloud_placeholder: boolean;
  is_raw: boolean;
  group_id: string | null;
  needs_ai: boolean;
}

export interface SortCriteria {
  key: string;
  label?: string;
  order: string;
}

export interface SupportedTypes {
  nonRaw: Array<string>;
  raw: Array<string>;
}

export interface Progress {
  completed?: number;
  current?: number;
  stage?: string;
  total: number;
}

export type PanoramaProjection = 'perspective' | 'cylindrical' | 'spherical' | 'panini' | 'stereographic';

export type PanoramaProjectionMode = 'auto' | PanoramaProjection;

export type MergeKind = 'panorama' | 'hdr' | 'focusStack';

export type HdrToneProfile = 'natural' | 'interior' | 'dramatic' | 'vivid' | 'agxScene' | 'localLaplacian';
export type HdrDeghostSensitivity = 'off' | 'low' | 'medium' | 'high';
export type HdrDeghostMode = 'fast' | 'opticalFlow';
export type HdrPreviewMode = 'sdr' | 'hdr10' | 'headroom';

export interface MergeBrushStroke {
  points: [number, number][];
  radiusNorm: number;
  frameIndex?: number | null;
  seamRole?: 'keep' | 'exclude' | null;
}

export interface PanoramaSettings {
  autoCrop: boolean;
  boundaryWarp: number;
  exposureCompensation: boolean;
  projection: PanoramaProjectionMode;
  yawDeg?: number;
  pitchDeg?: number;
  rollDeg?: number;
  fastDraft?: boolean;
  halfSize?: boolean;
  applyEdits?: boolean;
  hdrPanorama?: boolean | null;
  hdrProfile?: HdrToneProfile;
  hdrStrength?: number;
  exposureBias?: number;
  highlightRecovery?: number;
  shadowLift?: number;
  detailBoost?: number;
  outputFormat?: MergeOutputFormat;
  brushStrokes?: MergeBrushStroke[];
}

export type MergeOutputFormat = 'tiff' | 'jpeg' | 'ultrahdr' | 'png' | 'dng';

export interface HdrSettings {
  autoCrop: boolean;
  halfSize?: boolean;
  applyEdits?: boolean;
  profile?: HdrToneProfile;
  hdrStrength?: number;
  deghostSensitivity?: HdrDeghostSensitivity;
  deghostMode?: HdrDeghostMode;
  previewMode?: HdrPreviewMode;
  exposureBias?: number;
  highlightRecovery?: number;
  shadowLift?: number;
  detailBoost?: number;
  outputFormat?: MergeOutputFormat;
  brushStrokes?: MergeBrushStroke[];
}

export type FocusStackMethod = 'detail' | 'smooth' | 'depthMap';

export interface FocusStackSettings {
  autoCrop: boolean;
  method: FocusStackMethod;
  halfSize?: boolean;
  outputFormat?: MergeOutputFormat;
  brushStrokes?: MergeBrushStroke[];
}

export interface MergeSettingsByKind {
  panorama: PanoramaSettings;
  hdr: HdrSettings;
  focusStack: FocusStackSettings;
}

export type MergeSettings = MergeSettingsByKind[MergeKind];

export const DEFAULT_MERGE_SETTINGS: MergeSettingsByKind = {
  panorama: {
    autoCrop: true,
    boundaryWarp: 0,
    exposureCompensation: true,
    projection: 'auto',
    yawDeg: 0,
    pitchDeg: 0,
    rollDeg: 0,
    fastDraft: false,
    halfSize: false,
    applyEdits: true,
    hdrPanorama: null,
    hdrProfile: 'natural',
    hdrStrength: 100,
    exposureBias: 0,
    highlightRecovery: 50,
    shadowLift: 30,
    detailBoost: 1.05,
    outputFormat: 'tiff',
  },
  hdr: {
    autoCrop: true,
    halfSize: false,
    applyEdits: true,
    profile: 'natural',
    hdrStrength: 100,
    deghostSensitivity: 'medium',
    deghostMode: 'opticalFlow',
    previewMode: 'sdr',
    exposureBias: 0,
    highlightRecovery: 50,
    shadowLift: 30,
    detailBoost: 1.05,
    outputFormat: 'tiff',
  },
  focusStack: {
    autoCrop: true,
    method: 'detail',
    halfSize: false,
    outputFormat: 'tiff',
  },
};

export interface MergePreview {
  base64: string;
  excludedCount: number;
  fovDeg?: number | null;
  hdrPeakNits?: number | null;
  hdrPreviewBase64?: string | null;
  headroomMapBase64?: string | null;
  height: number;
  imageCount: number;
  isSceneReferred?: boolean;
  overlayBase64: string | null;
  referenceBase64?: string | null;
  projection: PanoramaProjection | null;
  projectionNote?: string | null;
  unalignedCount: number;
  width: number;
  wraps?: boolean;
}

export interface CullingSettings {
  similarityThreshold: number;
  blurThreshold: number;
  groupSimilar: boolean;
  filterBlurry: boolean;
}

interface ImageAnalysisResult {
  path: string;
  qualityScore: number;
  sharpnessMetric: number;
  centerFocusMetric: number;
  exposureMetric: number;
  width: number;
  height: number;
}

interface CullGroup {
  representative: ImageAnalysisResult;
  duplicates: ImageAnalysisResult[];
}

export interface CullingSuggestions {
  similarGroups: CullGroup[];
  blurryImages: ImageAnalysisResult[];
  failedPaths: string[];
}

export type AlbumItem = Album | AlbumGroup;

export interface Album {
  type: 'album';
  id: string;
  name: string;
  icon?: string;
  images: string[];
}

export interface AlbumGroup {
  type: 'group';
  id: string;
  name: string;
  icon?: string;
  children: AlbumItem[];
}

export interface FolderNode {
  children: FolderNode[];
  isDir: boolean;
  name: string;
  path: string;
  imageCount?: number;
  hasSubdirs?: boolean;
  modified?: number;
  created?: number;
}

export interface ColumnWidths {
  thumbnail: number;
  name: number;
  date: number;
  rating: number;
  color: number;
  shutter: number;
  aperture: number;
  iso: number;
  focal: number;
}
