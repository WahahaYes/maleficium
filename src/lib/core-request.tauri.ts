// core-request.tauri.ts — desktop transport for the core operation
// contract: one generic Tauri command carrying every JSON operation.

import { invoke } from '@tauri-apps/api/core';
import type { OpParams, OpResult, Request, Response } from './generated/api';

/** Run one core operation; rejects with the core's error string. */
export async function request<K extends keyof OpResult>(
  op: K,
  params: OpParams[K],
): Promise<OpResult[K]> {
  const req = { op, params } as Request;
  const resp = await invoke<Response>('core_request', { req });
  const got = (resp as { op: string }).op;
  if (got !== op) throw new Error(`backend returned ${got} for ${String(op)}`);
  return (resp as unknown as { result: OpResult[K] }).result;
}
