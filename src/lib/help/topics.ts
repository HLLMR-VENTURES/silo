// Help topics (#250): short answers Help shows and searches. `aliases` are other words people
// use for the same thing ("loadouts" finds Mod sets), matched like the title.
//   searchTopics("loadouts") // → [Mod sets]

export interface HelpTopic {
  id: string;
  title: string;
  body: string;
  aliases?: string[];
}

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: "active-parked",
    title: "Active and parked mods",
    body: "Active mods are in the game's mods folder, so Farming Simulator loads them. Parked mods are kept by Silo and not loaded. Parking a mod never deletes it; switch it back on any time.",
    aliases: ["enable", "disable", "turn on", "turn off", "switch"],
  },
  {
    id: "organize",
    title: "Organizing your library",
    body: "Organizing files your loose mod zips into category folders in your library archive. Active mods stay active: Silo links them back into your mods folder (or copies them, when the folder is cloud-synced). Folder mods (unzipped) are left where they are. Turn on Auto-file new mods in Settings → Library to file new downloads as they appear.",
    aliases: ["file", "auto-file", "archive", "categories", "sort"],
  },
  {
    id: "undo-organizing",
    title: "Undo organizing",
    body: "Settings → Maintenance → Undo organizing puts every mod back into your mods folder the way it was before Silo filed it. Nothing is deleted.",
    aliases: ["restore", "flatten", "vanilla", "put back"],
  },
  {
    id: "mod-sets",
    title: "Mod sets",
    body: "A mod set is a saved list of active mods. Save your active mods as a set, switch between sets for different farms, or share a set so friends can match it for multiplayer.",
    aliases: ["loadouts", "loadout", "collections", "profiles", "presets"],
  },
  {
    id: "multiplayer",
    title: "Playing multiplayer",
    body: "The host shares their mods as a Multiplayer check file (Mod sets → Share). Each player opens it in Mod sets → Open a shared set and presses Check my mods. Silo lists the mods that are missing, in a different version or extra.",
    aliases: ["mp", "server", "sync", "friends"],
  },
  {
    id: "updates",
    title: "Checking for updates",
    body: "Updates checks your mods against Silo's catalog of ModHub and GitHub releases. Silo installs GitHub updates for you; for ModHub, it opens the mod's page so you can download it there.",
    aliases: ["outdated", "new version", "modhub", "github"],
  },
  {
    id: "crash",
    title: "When the game crashes",
    body: "When your last game ended unexpectedly, Silo tells you as soon as you come back and names the mod that most likely broke it, with Park it. Diagnose → Last run shows the whole log reading. If the log can't tell, Find the problem mod parks half your mods at a time until it finds the one, and puts your mods back afterwards.",
    aliases: ["crash", "log", "bisect", "bisection", "find the problem mod", "error", "broken", "won't start"],
  },
  {
    id: "mods-folder",
    title: "Your mods folder",
    body: "Silo uses the folder Farming Simulator 25 reads, including a custom folder set in the game. Choose another folder in Settings → Mods folder.",
    aliases: ["folder", "path", "override", "documents"],
  },
  {
    id: "archive",
    title: "Where parked mods live",
    body: "Parked mods live in your library archive, inside your mods folder by default. Settings → Library can move it to another drive, which is useful for a big library on a small SSD.",
    aliases: ["drive", "disk space", "ssd", "move library"],
  },
];

/** Topics whose title, aliases or text contain every word of `query` (case-insensitive). */
export function searchTopics(query: string, topics: HelpTopic[] = HELP_TOPICS): HelpTopic[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return topics;
  return topics.filter((t) => {
    const hay = [t.title, t.body, ...(t.aliases ?? [])].join(" ").toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
