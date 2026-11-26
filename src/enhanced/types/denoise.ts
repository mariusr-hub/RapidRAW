export interface DenoiseRoiPreview {
  originalBase64: string;
  denoisedBase64: string;
  lumaOriginalBase64: string;
  lumaDenoisedBase64: string;
  chromaOriginalBase64: string;
  chromaDenoisedBase64: string;
  residualBase64: string;
  x0: number;
  y0: number;
  size: number;
  fullWidth: number;
  fullHeight: number;
  effectiveMethod: string;
}
