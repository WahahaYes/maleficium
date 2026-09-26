import { describe, expect, it } from 'vitest';
import {
  BUILTIN_HEADING,
  gallerySections,
  templateId,
  USER_CATEGORY,
  type TemplateInfo,
} from './templates';

const t = (id: string, name: string, category: string, user = false): TemplateInfo => ({
  id,
  name,
  description: '',
  category,
  main: 'main.tex',
  user,
});

describe('gallerySections', () => {
  it("puts built-in templates under their own heading by category, then the user's", () => {
    const sections = gallerySections([
      t('article', 'Article', 'Papers'),
      t('report', 'Report', 'Long documents'),
      t('journal', 'Journal Paper', 'Papers'),
      t('mine', 'My Thesis', 'Anything', true),
      t('book', 'Book', 'Long documents'),
    ]);
    expect(sections.map((s) => s.heading)).toEqual([BUILTIN_HEADING, USER_CATEGORY]);
    const [builtin, own] = sections;
    expect(builtin.groups.map((g) => g.category)).toEqual(['Papers', 'Long documents']);
    expect(builtin.groups[0].items.map((i) => i.id)).toEqual(['article', 'journal']);
    expect(builtin.groups[1].items.map((i) => i.id)).toEqual(['book', 'report']);
    expect(own.groups.flatMap((g) => g.items.map((i) => i.id))).toEqual(['mine']);
  });

  it('leaves out a section with no templates', () => {
    expect(gallerySections([t('article', 'Article', 'Papers')]).map((s) => s.heading)).toEqual([
      BUILTIN_HEADING,
    ]);
    expect(gallerySections([t('mine', 'Mine', 'Anything', true)]).map((s) => s.heading)).toEqual([
      USER_CATEGORY,
    ]);
    expect(gallerySections([])).toEqual([]);
  });
});

describe('templateId', () => {
  it('slugs a display name into a valid id', () => {
    expect(templateId('My Lab Report (2024)')).toBe('my-lab-report-2024');
    expect(templateId('  Thèse — Version 2 ')).toBe('these-version-2');
    expect(templateId('!!!')).toBe('');
    expect(templateId('x'.repeat(80))).toHaveLength(48);
  });
});
