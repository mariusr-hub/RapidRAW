export interface G2vCalibrationResult {
  gainR: number;
  gainG: number;
  gainB: number;
  bgR: number;
  bgG: number;
  bgB: number;
  starCount: number;
  colorTempKelvin: number;
  message: string;
}

export interface StarSeparationExportResult {
  starlessPath: string;
  starsPath: string;
  engine: string;
  message: string;
}
