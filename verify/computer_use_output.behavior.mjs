import assert from 'node:assert/strict';
import { formatWorkExecutionOutput } from '../src/services/workOutputFormatter.ts';
import { describeWorkTaskError } from '../src/services/tasks.ts';

const output = (status, stopReason, stepsUsed = 1) => ({
  skillInvocation: {
    capability: 'computer.use.execute',
    output: { status, stopReason, stepsUsed },
  },
});

assert.equal(
  formatWorkExecutionOutput(output('completed', 'completed')),
  'Computer Use 已完成（1 步）。',
);
assert.match(
  formatWorkExecutionOutput(output('stopped', 'stepLimit', 3)),
  /未完成（stepLimit）/,
);
assert.doesNotMatch(
  formatWorkExecutionOutput(output('stopped', 'stepLimit', 3)),
  /已完成/,
);
assert.equal(
  describeWorkTaskError('[ExecutionFailed] Computer Use was cancelled', 'task'),
  'Computer Use 已取消，未继续操作。',
);
console.log('PASS: Computer Use result presentation');
