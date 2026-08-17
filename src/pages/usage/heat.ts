export const heat = {
  ok: "#3b9a55",
  warn: "#d9a21b",
  crit: "#d4524c"
} as const;

export type HeatLevel = keyof typeof heat;

export function planHeat(percent: number): HeatLevel {
  if (percent >= 90) return "crit";
  if (percent >= 70) return "warn";
  return "ok";
}

export function weekHeat(requests: number, weeklyMax: number): HeatLevel {
  if (weeklyMax <= 0) return "ok";
  const percent = requests / weeklyMax * 100;
  if (percent >= 85) return "crit";
  if (percent >= 60) return "warn";
  return "ok";
}
