import { useEffect } from "react";
import { accentToBubble, applyThemeVars } from "../lib/color";
import { tauriApi } from "../services/tauriApi";
import { normalizeConfig, useConfigStore } from "../store/configStore";

/** Loads persisted configuration and keeps document theme variables in sync. */
export function useAppConfiguration() {
  const config = useConfigStore((state) => state.config);
  const setConfig = useConfigStore((state) => state.setConfig);

  useEffect(() => {
    tauriApi
      .getConfig()
      .then((rawConfig) => setConfig(normalizeConfig(rawConfig)))
      .catch(console.error);
  }, [setConfig]);

  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--accent", config.accent_color);
    root.style.setProperty(
      "--msg-user-bg",
      config.msg_user_bg || accentToBubble(config.accent_color, 0.12, config.bg_color),
    );
    root.style.setProperty(
      "--msg-user-border",
      config.msg_user_border || accentToBubble(config.accent_color, 0.18, config.bg_color),
    );
    applyThemeVars(config.bg_color, config.accent_color, config.theme_mode);
  }, [
    config.accent_color,
    config.bg_color,
    config.msg_user_bg,
    config.msg_user_border,
    config.theme_mode,
  ]);
}
