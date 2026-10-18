// silo://collection?url=… links (the "Open in Silo" button on a shared set's web page) open
// Mod sets → Open a shared set with a preview; they never download or switch anything.
// App starts listening before any other startup work, so a link that arrives while Silo is still
// loading isn't lost, then reads the link Silo was launched with (a cold start from a link).
//   const stop = listenForSharedSets((url) => openShared(url), { onOpenUrl, getCurrent });
import { parseCollectionDeepLink } from "../api";

export interface DeepLinkApi {
  onOpenUrl: (handler: (urls: string[]) => void) => Promise<() => void>;
  getCurrent: () => Promise<string[] | null | undefined>;
}

/**
 * The first shared-set link in a batch of opened URLs.
 * @param urls what the OS handed over
 * @return the link to preview, or null when none is a silo://collection link
 */
export function sharedSetLink(urls: readonly string[] | null | undefined): string | null {
  for (const raw of urls ?? []) {
    const target = parseCollectionDeepLink(raw);
    if (target) return target;
  }
  return null;
}

/**
 * Listen for shared-set links: first for links while running, then the launch link.
 * Outside the desktop app (tests, a plain browser) the API throws and nothing is registered.
 * @param open called with each link to preview
 * @param api the deep-link plugin's functions
 * @return stops listening
 */
export async function listenForSharedSets(open: (url: string) => void, api: DeepLinkApi): Promise<() => void> {
  let stop: () => void = () => {};
  const handle = (urls: readonly string[] | null | undefined) => {
    const url = sharedSetLink(urls);
    if (url) open(url);
  };
  try {
    stop = await api.onOpenUrl(handle);
  } catch {
    /* not a desktop deep-link context */
  }
  try {
    handle(await api.getCurrent());
  } catch {
    /* not a desktop deep-link context */
  }
  return stop;
}
