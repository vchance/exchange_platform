import { createI18n, wordingFor } from '@yuppers/shared';

import { recordOf } from '../../__tests__/fake-record';
import { fakeService } from '../../__tests__/fake-service';
import { recordHtml } from '../record-html';

/*
 * The page the device prints to a PDF of the record: the plain summary
 * first, then the record in full, and nothing in it that could run or reach
 * anywhere, whatever the parties wrote.
 */

const en = createI18n('en', wordingFor('en'), () => {});
const es = createI18n('es', wordingFor('es'), () => {});

function record() {
  const service = fakeService();
  return recordOf(service);
}

test('the summary comes first, then the record in full', () => {
  const html = recordHtml(record(), en);
  const w = en.wording.record;
  const order = [
    'Record PVVS-5Q2K',
    w.summary.heading,
    'Between Ana Ruiz and Ben Ortiz.',
    w.summaryHeading,
    w.aboutHeading,
    w.itemsHeading,
    'Version 1',
    w.signaturesHeading,
    w.eventsHeading,
  ].map((text) => html.indexOf(text));
  expect(order.every((at) => at >= 0)).toBe(true);
  expect([...order].sort((a, b) => a - b)).toEqual(order);
  expect(html).toContain('Amount: $450.00');
  expect(html).toContain('Fingerprint of these terms: ' + 'ab'.repeat(32));
});

test('what the parties wrote is escaped and set apart, and the page can run nothing', () => {
  const found = record();
  const hostile = '<script>alert(1)</script><img src="https://example.test/x">';
  found.revisions[0].signed.terms = hostile;
  found.revisions[0].signed.contributions[0].description = `A & B ${hostile}`;
  const html = recordHtml(found, en);
  expect(html).not.toContain('<script>');
  expect(html).not.toContain('<img');
  expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
  expect(html).toContain('<p class="written" dir="auto">A &amp; B &lt;script&gt;');
  expect(html).toContain(`content="default-src 'none'; style-src 'unsafe-inline'"`);
});

test('in the reader’s language, with every message filled in', () => {
  const html = recordHtml(record(), es);
  expect(html).toContain('<html lang="es" dir="ltr">');
  expect(html).toContain(es.wording.record.summary.heading);
  expect(html.replace(/<style>[\s\S]*<\/style>/, '')).not.toMatch(/[{}]/);
});
