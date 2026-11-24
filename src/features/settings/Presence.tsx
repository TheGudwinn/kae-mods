import { useTranslation } from "react-i18next";
import { setPresenceOn, usePresenceAvailable, usePresenceOn } from "../../lib/presence";

/** The Discord status switch. Hidden until a Discord application id is configured. */
export default function Presence() {
  const { t } = useTranslation();
  const on = usePresenceOn();
  const available = usePresenceAvailable();
  if (!available) return null;
  return (
    <fieldset className="block">
      <legend>{t("presence.title")}</legend>
      <p className="note">{t("presence.hint")}</p>
      <label className="check">
        <input type="checkbox" checked={on} onChange={(e) => setPresenceOn(e.target.checked)} />
        <span>{t("presence.show")}</span>
      </label>
    </fieldset>
  );
}
