export interface CameraProfileInfo {
  id: string;
  name: string;
  cameraModel: string;
  format: string;
  path: string;
  dualIlluminant: boolean;
  hasHueSatMap: boolean;
  hasToneCurve: boolean;
}

export interface SoftProofConfig {
  enabled: boolean;
  target: string;
  intent: string;
  oogWarning: boolean;
  customIccPath?: string | null;
}
