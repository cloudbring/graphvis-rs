/* tslint:disable */
/* eslint-disable */

/**
 * Parses and lays out a DOT graph and returns JSON statistics (`nodes`, `edges`, `subgraphs`, `width_pt`, `height_pt`).
 */
export function inspectGraph(dot_source: string, engine: string): string;

/**
 * Renders a DOT string using the specified layout engine (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`)
 * and output format (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).
 */
export function render(dot_source: string, engine: string, format: string): string;

/**
 * Convenience helper to render a DOT graph to Graphviz `json` format.
 */
export function renderJson(dot_source: string, engine: string): string;

/**
 * Convenience helper to render a DOT graph directly to an SVG string using the specified layout engine.
 */
export function renderSvg(dot_source: string, engine: string): string;

/**
 * Returns the list of supported layout engines as a JSON array.
 */
export function supportedEngines(): string;

/**
 * Returns the list of supported output formats as a JSON array.
 */
export function supportedFormats(): string;

/**
 * Returns the version of `graphvis-rs` (compatible with Graphviz 16.1.0).
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly inspectGraph: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly render: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly renderJson: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly renderSvg: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly supportedEngines: () => [number, number];
    readonly supportedFormats: () => [number, number];
    readonly version: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
