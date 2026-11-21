import { openExternal } from "./open";

export const REPO = "TheGudwinn/kae-mods";
const ISSUE_LIMIT = 6000; // longer addresses are cut by browsers and servers

/** Opens a new issue of the project's repository with the text filled in. Returns false when the text was too long to carry in the address. */
export async function openIssue(title: string, body: string): Promise<boolean> {
  const url = `https://github.com/${REPO}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
  const fits = url.length <= ISSUE_LIMIT;
  await openExternal(fits ? url : `https://github.com/${REPO}/issues/new?title=${encodeURIComponent(title)}`);
  return fits;
}
