// Small dependency-free test harness using the project's existing TypeScript compiler.
// Run from the workspace root: node --test tools/activity/ui-labels.test.mjs
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import ts from 'typescript'

const source = readFileSync(new URL('../../src/lib/activityClassification.ts', import.meta.url), 'utf8')
const output = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 },
}).outputText
const { activityCategoryLabel, activityPredictionText } = await import(
  `data:text/javascript;base64,${Buffer.from(output).toString('base64')}`
)

test('legacy events remain quiet', () => {
  assert.equal(activityPredictionText(), null)
  assert.equal(activityPredictionText(null), null)
})

test('accepted labels remain explicitly predictions', () => {
  assert.equal(activityPredictionText({ category: 'coding_debugging', uncertain: false }),
    'Predicted activity: Coding & debugging')
})

test('legacy v1 abstentions do not suggest Other at high or low confidence', () => {
  for (const confidence of [0.99, 0.05]) {
    assert.equal(activityPredictionText({
      category: 'other', confidence, uncertain: true, model_version: 'synthetic-bootstrap-v1',
    }), 'Activity unclear · insufficient evidence')
  }
})

test('v2 winners remain tentative when uncertain, without confidence percentages', () => {
  const labels = {
    coding_debugging: 'Coding & debugging',
    research_learning: 'Research & learning',
    writing_documentation: 'Writing & documentation',
    communication: 'Communication',
    planning_administration: 'Planning & administration',
    entertainment_media: 'Entertainment & media',
    system_navigation: 'System navigation',
  }
  for (const [category, label] of Object.entries(labels)) {
    for (const confidence of [0.99, 0.05]) {
      assert.equal(activityPredictionText({
        category, confidence, uncertain: true, model_version: 'synthetic-bootstrap-v2',
      }), `Activity unclear · possible ${label}`)
    }
  }
})

test('accepted Other is a prediction; uncertain Other is generic', () => {
  for (const uncertain of [false, true]) {
    assert.equal(activityPredictionText({
      category: 'other', confidence: 0.9, uncertain, model_version: 'synthetic-bootstrap-v2',
    }), uncertain ? 'Activity unclear' : 'Predicted activity: Other activity')
  }
})

test('unknown category identifiers are never rendered', () => {
  for (const category of ['unrecognized-v2', '__proto__', 'constructor']) {
    assert.equal(activityCategoryLabel(category), 'Activity unclear')
    for (const uncertain of [false, true]) {
      assert.equal(activityPredictionText({
        category, confidence: 0.99, uncertain, model_version: 'synthetic-bootstrap-v2',
      }), 'Activity unclear')
    }
  }
})

test('missing or null legacy fields cannot become accepted predictions', () => {
  for (const category of [undefined, null, 42, {}, ['other']]) {
    assert.equal(activityCategoryLabel(category), 'Activity unclear')
    assert.equal(activityPredictionText({ category, uncertain: false }), 'Activity unclear')
  }
  for (const uncertain of [undefined, null, 'false', 'true', 0, 1]) {
    assert.equal(activityPredictionText({ category: 'coding_debugging', uncertain }),
      'Activity unclear')
  }
  for (const model_version of [undefined, null, 42, '']) {
    assert.equal(activityPredictionText({ category: 'coding_debugging', uncertain: true, model_version }),
      'Activity unclear')
  }
  assert.equal(activityPredictionText({}), 'Activity unclear')
  assert.equal(activityPredictionText('other'), 'Activity unclear')
})
