import { useTranslation } from "react-i18next";
import GeneralSettings from "../minecraft/GeneralSettings";

/** What all Minecraft packs share, kept with the other settings too. */
export default function McSettings() {
  const { t } = useTranslation();
  return (
    <fieldset className="block">
      <legend>Minecraft</legend>
      <p className="note">{t("mc.generalHint")}</p>
      <GeneralSettings />
    </fieldset>
  );
}
