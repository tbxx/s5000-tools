import type { ParamInfo } from "./api";
import { format } from "./format";

/** A multi or part value as text: as `format`, with pan as the sampler shows it (L50 … MID … R50). */
export function formatPart(info: ParamInfo, v: number): string {
  if (info.id === "multi_part.pan") return v === 0 ? "MID" : v < 0 ? `L${-v}` : `R${v}`;
  return format(info, v);
}
