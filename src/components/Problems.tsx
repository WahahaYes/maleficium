// Problems.tsx — compatibility re-export.
//
// The Problems/Log/Terminal tab trio was unified into the single `LogStream`
// event stream. `parseLog` now lives in `lib/parseLog.ts`; this module keeps
// existing imports (`Problems.test.ts`, `scale.test.ts`) working.

export { parseLog, type ParsedLine } from '../lib/parseLog';
export { default } from './ProblemsView';
