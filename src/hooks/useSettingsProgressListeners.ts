import { useEffect, useRef } from "react";
import type {
  GeoIpDownloadProgressEvent,
  KernelListProgressEvent,
  UpdateCheckProgressEvent,
  UpdateDownloadProgressEvent,
} from "../types/settings";
import {
  listenGeoIpDownloadProgress,
  listenKernelListProgress,
  listenUpdateCheckProgress,
  listenUpdateDownloadProgress,
} from "../api/settings";

interface UseSettingsProgressListenersOptions {
  onGeoIpProgress?: (event: GeoIpDownloadProgressEvent) => void;
  onKernelListProgress?: (event: KernelListProgressEvent) => void;
  onUpdateCheckProgress?: (event: UpdateCheckProgressEvent) => void;
  onUpdateDownloadProgress?: (event: UpdateDownloadProgressEvent) => void;
}

export function useSettingsProgressListeners(
  options: UseSettingsProgressListenersOptions = {}
) {
  const {
    onGeoIpProgress,
    onKernelListProgress,
    onUpdateCheckProgress,
    onUpdateDownloadProgress,
  } = options;

  const ipDbAlertIdRef = useRef<string | null>(null);
  const updateCheckAlertIdRef = useRef<string | null>(null);
  const updateDownloadAlertIdRef = useRef<string | null>(null);

  useEffect(() => {
    let mounted = true;
    let geoIpDisposer: (() => void) | null = null;
    let kernelListDisposer: (() => void) | null = null;
    let updateCheckDisposer: (() => void) | null = null;
    let updateDownloadDisposer: (() => void) | null = null;

    async function bindListeners() {
      geoIpDisposer = await listenGeoIpDownloadProgress((event) => {
        if (!mounted) return;
        onGeoIpProgress?.(event);
      });

      kernelListDisposer = await listenKernelListProgress((event) => {
        if (!mounted) return;
        onKernelListProgress?.(event);
      });

      updateCheckDisposer = await listenUpdateCheckProgress((event) => {
        if (!mounted) return;
        onUpdateCheckProgress?.(event);
      });

      updateDownloadDisposer = await listenUpdateDownloadProgress((event) => {
        if (!mounted) return;
        onUpdateDownloadProgress?.(event);
      });
    }

    void bindListeners();

    return () => {
      mounted = false;
      if (geoIpDisposer) geoIpDisposer();
      if (kernelListDisposer) kernelListDisposer();
      if (updateCheckDisposer) updateCheckDisposer();
      if (updateDownloadDisposer) updateDownloadDisposer();
    };
  }, [onGeoIpProgress, onKernelListProgress, onUpdateCheckProgress, onUpdateDownloadProgress]);

  return { ipDbAlertIdRef, updateCheckAlertIdRef, updateDownloadAlertIdRef };
}
