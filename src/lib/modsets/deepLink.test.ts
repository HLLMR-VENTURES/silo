// silo://collection deep links (#251): heard before the launch link is read, routed to a preview,
// other links ignored, and a missing plugin (tests, a browser) never throws.
import { describe, expect, it, vi } from "vitest";
import { listenForSharedSets, sharedSetLink } from "./deepLink";

const link = "silo://collection?url=https%3A%2F%2Fgist.github.com%2Fu%2F0123456789abcdef0123456789abcdef";
const gist = "https://gist.github.com/u/0123456789abcdef0123456789abcdef";

describe("sharedSetLink", () => {
  it("takes the first silo://collection link", () => {
    expect(sharedSetLink(["https://example.com", link])).toBe(gist);
    expect(sharedSetLink(["silo://other?url=x", "not a url"])).toBeNull();
    expect(sharedSetLink(null)).toBeNull();
  });
});

describe("listenForSharedSets", () => {
  it("registers the listener before reading the launch link, and routes both", async () => {
    const order: string[] = [];
    let handler: ((urls: string[]) => void) | undefined;
    const stop = vi.fn();
    const open = vi.fn();
    const off = await listenForSharedSets(open, {
      onOpenUrl: async (h) => {
        order.push("listen");
        handler = h;
        return stop;
      },
      getCurrent: async () => {
        order.push("launch");
        return [link];
      },
    });
    expect(order).toEqual(["listen", "launch"]);
    expect(open).toHaveBeenCalledWith(gist);
    handler!(["silo://collection?url=abc"]);
    expect(open).toHaveBeenLastCalledWith("abc");
    off();
    expect(stop).toHaveBeenCalled();
  });

  it("does nothing outside the desktop app", async () => {
    const open = vi.fn();
    const off = await listenForSharedSets(open, {
      onOpenUrl: () => Promise.reject(new Error("no plugin")),
      getCurrent: () => Promise.reject(new Error("no plugin")),
    });
    expect(open).not.toHaveBeenCalled();
    expect(() => off()).not.toThrow();
  });
});
