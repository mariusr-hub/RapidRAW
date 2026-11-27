import { AppSettings } from '../types/settings';

export function getDefaultWgpuRendererSetting(osPlatform?: string | null): boolean {
  return osPlatform !== 'windows' && osPlatform !== 'linux' && osPlatform !== 'android';
}

export function isDirectWgpuSwapchainActive(
  appSettings: Pick<AppSettings, 'useWgpuRenderer'> | null | undefined,
  osPlatform?: string | null,
  isWgpuSupported?: boolean,
): boolean {
  if (isWgpuSupported === false) {
    return false;
  }
  if (osPlatform === 'android') {
    return false;
  }
  if (appSettings?.useWgpuRenderer !== undefined) {
    return appSettings.useWgpuRenderer;
  }
  return getDefaultWgpuRendererSetting(osPlatform);
}
