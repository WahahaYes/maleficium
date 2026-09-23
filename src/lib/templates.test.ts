import { describe, expect, it } from 'vitest';
import { groupTemplates, templateId, USER_CATEGORY, type TemplateInfo } from './templates';

const t = (id: string, name: string, category: string, user = false): TemplateInfo => ({
  id,
  name,
  description: '',
  category,
  main: 'main.tex',
  user,
});

describe('groupTemplates', () => {
  it('keeps bundled categories in order, sorts within, and puts user templates last', () => {
    const groups = groupTemplates([
      t('article', 'Article', 'Papers'),
      t('report', 'Report', 'Long documents'),
      t('journal', 'Journal Paper', 'Papers'),
      t('mine', 'My Thesis', 'Anything', true),
      t('book', 'Book', 'Long documents'),
    ]);
    expect(groups.map((g) => g.category)).toEqual(['Papers', 'Long documents', USER_CATEGORY]);
    expect(groups[0].items.map((i) => i.id)).toEqual(['article', 'journal']);
    expect(groups[1].items.map((i) => i.id)).toEqual(['book', 'report']);
    expect(groups[2].items.map((i) => i.id)).toEqual(['mine']);
  });

  it('is empty for no templates', () => {
    expect(groupTemplates([])).toEqual([]);
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
