export interface LensDistortionParams {
  k1: number;
  k2: number;
  k3: number;
  model?: number;
  tca_vr?: number;
  tca_vb?: number;
  vig_k1?: number;
  vig_k2?: number;
  vig_k3?: number;
  enabled?: boolean;
}

export interface EnhancedAdjustments {
  contrastCleaner: number;
  skinShield: number;
  foliageEnhancer: number;
  ortonSpread: number;
  lightsBrightness: number;
  lightsContrast: number;
  lightsDetails: number;
  lightsTemperature: number;
  lightsTint: number;
  lightsSaturation: number;
  lightsVibrance: number;
  supercontrastShadows: number;
  supercontrastMidtones: number;
  supercontrastHighlights: number;
  supercontrastPivots: number;
  cornerSharpnessBoost: number;
  bokehMaskProtection: number;
  microContrast: number;
  clearView: number;
  hlrEnabled: boolean;
  hlrClipCeiling: number;
  lensDistortionParams: LensDistortionParams | null;
  lensAutocropScale: number;
  defringeEnabled: boolean;
  defringePurpleAmount: number;
  defringeGreenAmount: number;
  defringeEdgeThreshold: number;
}

export interface GradingSnapshot {
  id: string;
  name: string;
  adjustments: Record<string, any>;
  timestamp: number;
}
