// Pins the committed paper-bundle manifest schema against the typed contract
// the frontend sees: the schema and the generated profile union are one list.
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const schema = JSON.parse(
  readFileSync('src-tauri/core/schemas/paper-bundle-1.schema.json', 'utf8'),
) as {
  $schema: string;
  $id: string;
  required: string[];
  additionalProperties: boolean;
  properties: Record<string, { enum?: string[]; const?: unknown }>;
  $defs: { widget: { properties: { type: { enum: string[] } } } };
};

/** The members of `export type <name> = "a" | "b";` in a generated module. */
function unionOf(source: string, name: string): string[] {
  const m = new RegExp(`export type ${name} = ([^;]+);`).exec(source);
  return m ? [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]) : [];
}

describe('paper bundle manifest schema', () => {
  const events = readFileSync('src/lib/generated/events.ts', 'utf8');
  const api = readFileSync('src/lib/generated/api.ts', 'utf8');

  it('is JSON Schema 2020-12 for format version 1 and closed at the top level', () => {
    expect(schema.$schema).toBe('https://json-schema.org/draft/2020-12/schema');
    expect(schema.properties.format.const).toBe('maleficium-paper-bundle');
    expect(schema.properties.formatVersion.const).toBe(1);
    expect(schema.additionalProperties).toBe(false);
    expect(schema.required).toEqual(
      expect.arrayContaining(['format', 'formatVersion', 'paper', 'pdf', 'theme', 'widgets']),
    );
  });

  it('lists the same profiles as the typed IPC contract', () => {
    expect(unionOf(events, 'BundleProfile')).toEqual(schema.properties.profile.enum);
  });

  it('lists the same widget types as the widget list', () => {
    expect(unionOf(api, 'WidgetType')).toEqual(schema.$defs.widget.properties.type.enum);
  });

  it('red control: a contract that drifted from the schema is detected', () => {
    const drifted = events.replace('"single-file"', '"singlefile"');
    expect(unionOf(drifted, 'BundleProfile')).not.toEqual(schema.properties.profile.enum);
  });
});
