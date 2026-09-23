// templates.ts — project templates over IPC, and the gallery's grouping.

import { invoke } from '@tauri-apps/api/core';

export type TemplateInfo = {
  id: string;
  name: string;
  description: string;
  category: string;
  /** Root-relative main file. */
  main: string;
  /** Saved or imported by the user, not bundled. */
  user: boolean;
};
export type TemplateList = { templates: TemplateInfo[]; unreadable: string[] };
/** A project made from a template: its absolute root and main file. */
export type Created = { root: string; main: string };

export const USER_CATEGORY = 'Your templates';

export async function listTemplates(): Promise<TemplateList> {
  return await invoke<TemplateList>('templates_list');
}

export async function instantiateTemplate(
  template: string,
  parent: string,
  name: string,
): Promise<Created> {
  return await invoke<Created>('template_instantiate', { template, parent, name });
}

export async function saveProjectAsTemplate(
  rootId: string,
  info: TemplateInfo,
): Promise<TemplateInfo> {
  return await invoke<TemplateInfo>('template_save_project', { rootId, info });
}

export async function importFolderAsTemplate(
  dir: string,
  info: TemplateInfo,
): Promise<TemplateInfo> {
  return await invoke<TemplateInfo>('template_import_folder', { dir, info });
}

export async function welcomeProject(): Promise<Created> {
  return await invoke<Created>('template_welcome');
}

/**
 * Gallery sections: bundled categories in first-seen order, then the user's
 * own templates last, each sorted by name.
 */
export function groupTemplates(
  list: TemplateInfo[],
): { category: string; items: TemplateInfo[] }[] {
  const order: string[] = [];
  const by = new Map<string, TemplateInfo[]>();
  for (const t of list) {
    const c = t.user ? USER_CATEGORY : t.category;
    if (!by.has(c)) {
      by.set(c, []);
      if (!t.user) order.push(c);
    }
    by.get(c)!.push(t);
  }
  if (by.has(USER_CATEGORY) && !order.includes(USER_CATEGORY)) order.push(USER_CATEGORY);
  return order.map((category) => ({
    category,
    items: [...by.get(category)!].sort((a, b) => a.name.localeCompare(b.name)),
  }));
}

/** A template id from a display name: lowercase letters, digits and dashes. */
export function templateId(name: string): string {
  return name
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
}
