// templates.ts — project templates over the operation contract, and the
// gallery's sections.

import { request } from './core-request.tauri';
import type { Created, TemplateInfo, TemplateList } from './generated/api';

export type { Created, TemplateInfo, TemplateList };

export const BUILTIN_HEADING = 'Built-in';
export const USER_CATEGORY = 'Your templates';

export async function listTemplates(): Promise<TemplateList> {
  return await request('templatesList', {});
}

export async function instantiateTemplate(
  template: string,
  parent: string,
  name: string,
): Promise<Created> {
  return await request('templateInstantiate', { template, parent, name });
}

export async function saveProjectAsTemplate(
  rootId: string,
  info: TemplateInfo,
): Promise<TemplateInfo> {
  return await request('templateSaveProject', { rootId, info });
}

export async function importFolderAsTemplate(
  dir: string,
  info: TemplateInfo,
): Promise<TemplateInfo> {
  return await request('templateImportFolder', { dir, info });
}

export async function welcomeProject(): Promise<Created> {
  return await request('templateWelcome', {});
}

/** One labeled part of the gallery: its heading and its category groups. */
export type GallerySection = {
  heading: string;
  /** Shown under the heading. */
  note: string;
  groups: { category: string; items: TemplateInfo[] }[];
};

/**
 * The gallery: built-in templates grouped by category in first-seen order,
 * then the user's own, each group sorted by name. Empty sections are left out.
 */
export function gallerySections(list: TemplateInfo[]): GallerySection[] {
  const order: string[] = [];
  const by = new Map<string, TemplateInfo[]>();
  const own: TemplateInfo[] = [];
  for (const t of list) {
    if (t.user) {
      own.push(t);
      continue;
    }
    if (!by.has(t.category)) {
      by.set(t.category, []);
      order.push(t.category);
    }
    by.get(t.category)!.push(t);
  }
  const byName = (items: TemplateInfo[]) => [...items].sort((a, b) => a.name.localeCompare(b.name));
  const sections: GallerySection[] = [];
  if (order.length > 0) {
    sections.push({
      heading: BUILTIN_HEADING,
      note: 'Included with Maleficium.',
      groups: order.map((category) => ({ category, items: byName(by.get(category)!) })),
    });
  }
  if (own.length > 0) {
    sections.push({
      heading: USER_CATEGORY,
      note: 'Saved or imported by you.',
      groups: [{ category: '', items: byName(own) }],
    });
  }
  return sections;
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
