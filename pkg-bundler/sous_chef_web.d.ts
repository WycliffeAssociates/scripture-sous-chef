/* tslint:disable */
/* eslint-disable */
/**
 * A finding as the editor sees it: UTF-16 ranges, string code/severity.
 */
export interface Finding {
    sid: string;
    code: string;
    severity: string;
    /**
     * UTF-16 code-unit offsets into the verse text.
     */
    start: number;
    end: number;
    score: number | null;
}

/**
 * The return type. TS: `Finding[]`.
 */
export type Findings = Finding[];

/**
 * `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
 */
export type VrefMap = Record<string, string>;


/**
 * Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
 * optional parallel map. Returns findings with UTF-16 ranges.
 */
export function analyze_vref(target: VrefMap, source?: VrefMap | null): Findings;
