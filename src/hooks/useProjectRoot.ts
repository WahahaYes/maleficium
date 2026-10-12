import { useCallback, useRef, useState } from 'react';
import { relUnder } from '../lib/paths';

// The open project's root folder and backend-minted id. The refs move at
// once on a switch, so async work started for the old root can tell it is
// stale before the next render.
export function useProjectRoot() {
  const [root, setRoot] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const rootRef = useRef<string | null>(root);
  rootRef.current = root;
  const projectIdRef = useRef<string | null>(projectId);
  projectIdRef.current = projectId;
  const setRootNow = useCallback((v: string | null) => {
    rootRef.current = v;
    setRoot(v);
  }, []);
  const setProjectIdNow = useCallback((v: string | null) => {
    projectIdRef.current = v;
    setProjectId(v);
  }, []);
  /** Project-relative path for a file inside the open project, else null. */
  const relInProject = useCallback((abs: string): string | null => {
    const r = rootRef.current;
    return r ? relUnder(r, abs) : null;
  }, []);
  return { root, projectId, rootRef, projectIdRef, setRootNow, setProjectIdNow, relInProject };
}
