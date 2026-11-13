import { describe, expect, it } from 'vitest';
import {
  buildParameters,
  cancelState,
  decisionState,
  exceedsTargetLimit,
  hasActiveJob,
  initialParamValues,
  isTerminalStatus,
  parameterField,
  parameterFields,
  runNeedsApproval,
  statusLabel,
} from '../utils/orchestration';
import type { BoltJobStatus } from '../types';

describe('job status helpers', () => {
  it('treats pending_approval, queued and running as not terminal', () => {
    const active: BoltJobStatus[] = ['pending_approval', 'queued', 'running'];
    const done: BoltJobStatus[] = [
      'succeeded',
      'partial',
      'failed',
      'cancelled',
      'timed_out',
      'rejected',
    ];
    active.forEach((s) => expect(isTerminalStatus(s)).toBe(false));
    done.forEach((s) => expect(isTerminalStatus(s)).toBe(true));
  });

  it('polls only while some job is active', () => {
    expect(hasActiveJob(undefined)).toBe(false);
    expect(hasActiveJob([{ status: 'failed' }, { status: 'succeeded' }])).toBe(false);
    expect(hasActiveJob([{ status: 'failed' }, { status: 'running' }])).toBe(true);
  });

  it('labels statuses and passes unknown ones through', () => {
    expect(statusLabel('timed_out')).toBe('Timed out');
    expect(statusLabel('weird')).toBe('weird');
  });
});

describe('parameterField type mapping', () => {
  const kind = (type: string) => parameterField('p', { type }).kind;

  it('maps Puppet types to field kinds', () => {
    expect(kind('String')).toBe('string');
    expect(kind('String[1]')).toBe('string');
    expect(kind('Integer')).toBe('integer');
    expect(kind('Integer[0, 10]')).toBe('integer');
    expect(kind('Float')).toBe('float');
    expect(kind('Boolean')).toBe('boolean');
    expect(kind('Array[String]')).toBe('array');
    expect(kind('Hash[String, Any]')).toBe('hash');
    expect(kind('Variant[String, Integer]')).toBe('json');
  });

  it('extracts Enum options, quoted or not', () => {
    const f = parameterField('mode', { type: 'Enum[\'start\', "stop", restart]' });
    expect(f.kind).toBe('enum');
    expect(f.options).toEqual(['start', 'stop', 'restart']);
  });

  it('marks Optional[...], a default and optional:true as optional', () => {
    expect(parameterField('p', { type: 'String' }).optional).toBe(false);
    expect(parameterField('p', { type: 'Optional[String]' })).toMatchObject({
      optional: true,
      kind: 'string',
    });
    expect(parameterField('p', { type: 'Integer', default: 3 }).optional).toBe(true);
    expect(parameterField('p', { type: 'Integer', optional: true }).optional).toBe(true);
  });

  it('flags sensitive parameters', () => {
    expect(parameterField('pw', { type: 'String', sensitive: true }).sensitive).toBe(true);
    expect(parameterField('pw', { type: 'String' }).sensitive).toBe(false);
  });

  it('lists required parameters first', () => {
    const fields = parameterFields({
      a: { type: 'Optional[String]' },
      z: { type: 'String' },
    });
    expect(fields.map((f) => f.name)).toEqual(['z', 'a']);
  });
});

describe('buildParameters', () => {
  const fields = parameterFields({
    name: { type: 'String' },
    count: { type: 'Optional[Integer]' },
    ratio: { type: 'Optional[Float]' },
    flag: { type: 'Boolean', default: true },
    mode: { type: 'Enum[a, b]', optional: true },
    list: { type: 'Optional[Array[String]]' },
    opts: { type: 'Optional[Hash]' },
  });

  it('requires required fields and leaves empty optional ones out', () => {
    const { params, errors } = buildParameters(fields, initialParamValues(fields));
    expect(errors).toEqual({ name: 'Required' });
    expect(params).toEqual({});
  });

  it('converts typed values', () => {
    const values = {
      ...initialParamValues(fields),
      name: 'web',
      count: '4',
      ratio: '0.5',
      flag: false,
      mode: 'b',
      list: '["x","y"]',
      opts: '{"k":1}',
    };
    const { params, errors } = buildParameters(fields, values);
    expect(errors).toEqual({});
    expect(params).toEqual({
      name: 'web',
      count: 4,
      ratio: 0.5,
      flag: false,
      mode: 'b',
      list: ['x', 'y'],
      opts: { k: 1 },
    });
  });

  it('reports invalid numbers, enum values and JSON', () => {
    const values = {
      ...initialParamValues(fields),
      name: 'x',
      count: '1.5',
      ratio: 'abc',
      mode: 'c',
      list: '{"not":"array"}',
      opts: '[',
    };
    const { errors } = buildParameters(fields, values);
    expect(Object.keys(errors).sort()).toEqual(['count', 'list', 'mode', 'opts', 'ratio']);
  });

  it('sends an optional boolean only when it differs from its default', () => {
    const f = parameterFields({ on: { type: 'Boolean', optional: true } });
    expect(buildParameters(f, { on: false }).params).toEqual({});
    expect(buildParameters(f, { on: true }).params).toEqual({ on: true });
  });
});

describe('approval and limits', () => {
  const limits = { max_targets_per_run: 10, approval_target_threshold: 3 };

  it('needs approval when flagged or above the threshold', () => {
    expect(runNeedsApproval({ requires_approval: true }, 1, limits)).toBe(true);
    expect(runNeedsApproval({ requires_approval: false }, 3, limits)).toBe(false);
    expect(runNeedsApproval({ requires_approval: false }, 4, limits)).toBe(true);
    expect(runNeedsApproval(null, 1, limits)).toBe(false);
  });

  it('blocks runs above the target maximum', () => {
    expect(exceedsTargetLimit(10, limits)).toBe(false);
    expect(exceedsTargetLimit(11, limits)).toBe(true);
  });

  it('never lets the requester decide their own job', () => {
    const job = { status: 'pending_approval' as const, requested_by_id: 'u1' };
    expect(decisionState(job, { userId: 'u1', canApprove: true, canCreate: true })).toMatchObject({
      visible: true,
      disabled: true,
    });
    expect(decisionState(job, { userId: 'u2', canApprove: true, canCreate: false })).toEqual({
      visible: true,
      disabled: false,
    });
    expect(decisionState(job, { userId: 'u2', canApprove: false, canCreate: true }).visible).toBe(
      false
    );
    expect(
      decisionState(
        { ...job, status: 'queued' },
        { userId: 'u2', canApprove: true, canCreate: true }
      ).visible
    ).toBe(false);
  });

  it('lets the requester or an approver cancel an unfinished job', () => {
    const job = { status: 'queued' as const, requested_by_id: 'u1' };
    expect(cancelState(job, { userId: 'u1', canApprove: false, canCreate: true }).visible).toBe(
      true
    );
    expect(cancelState(job, { userId: 'u2', canApprove: false, canCreate: true }).visible).toBe(
      false
    );
    expect(cancelState(job, { userId: 'u2', canApprove: true, canCreate: false }).visible).toBe(
      true
    );
    expect(
      cancelState(
        { ...job, status: 'succeeded' },
        { userId: 'u1', canApprove: true, canCreate: true }
      ).visible
    ).toBe(false);
  });
});
