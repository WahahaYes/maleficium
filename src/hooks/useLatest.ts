import { useRef, type MutableRefObject } from 'react';

// A ref that always holds the value from the latest render, for listeners
// subscribed once that must not close over stale state.
export function useLatest<T>(value: T): MutableRefObject<T> {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}
