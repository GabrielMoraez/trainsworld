// T-1792: whether a line carries trains at the demand level in force on its own clock. Every line
// bullet reads this (ui/widgets.tsx's Chip, map/trackTagRows.ts's lineBullet) and draws dimmed
// where a line is not running, so a line left without trains is not forgotten.

import { effect } from "@preact/signals";
import { t } from "../i18n";
import { computed } from "@preact/signals";
import { demandAt, lineSeconds } from "./clock";
import { lineCountry, rowIn } from "./holidays";
import { DEMAND_INDEX, type LineStats } from "./types";
import { linesById, restShown, world } from "./world";

/** A line's trains at level `lev` on its kind of day now in its own local time (weekdays, weekend
 * days or holidays, by its country's calendar; `restShown`). */
export function trainsNowAt(id: string, lev: number): number {
  const w = world.value, st = w?.lineStats[id], l = w ? linesById(w).get(id) : undefined;
  if (!st || !l) return 0;
  const row = rowIn(lineCountry(l.stops), Math.floor(lineSeconds(st) / 86400));
  return restShown(id, row)?.[lev] ?? 0;
}

/** The ids of the lines with no trains at their level now (T-1783's test: a train count of 0, or a
 * headway of 0), or that cannot carry any (a planned or broken line). Counts typed but not yet
 * applied are counted (`pendingTrains`). A line without figures yet is not listed. */
let last: ReadonlySet<string> = new Set();
export const idleLines = computed<ReadonlySet<string>>(() => {
  const w = world.value;
  const out = new Set<string>();
  if (w) {
    for (const id in w.lineStats) {
      const st: LineStats = w.lineStats[id];
      const lev = DEMAND_INDEX[demandAt(lineSeconds(st))];
      // a line whose weekend count is 0 is idle on a Saturday though its weekday count is not
      const n = trainsNowAt(id, lev);
      if (st.status !== "running" || !(n > 0) || st.autoS?.[lev] === 0) out.add(id);
    }
  }
  // the same ids keep the same set, so a bullet is drawn again only when its line changes
  if (last.size !== out.size || ![...out].every((id) => last.has(id))) last = out;
  return last;
});

/** Whether line `id` has no trains now. Read in a component, it draws again when that changes. */
export const isIdleLine = (id: string): boolean => idleLines.value.has(id);

/** The hover line a dimmed bullet adds. */
export let IDLE_TIP = "";
effect(() => { IDLE_TIP = t("lineRunning.idle"); });
