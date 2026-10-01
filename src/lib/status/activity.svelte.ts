// Background task state for the status light, fed only by core `task` events
// (plus one snapshot on load, for windows opened mid-task). No polling.

import { onCoreEvent, rpc } from '$lib/backend/rpc';
import type { TaskEvent } from '$lib/backend/types';
import { initialActivity, mergeSnapshot, reduceTask, type ActivityState } from './activity';

export const activity = $state<{ value: ActivityState }>({ value: initialActivity() });

let started = false;

export async function initActivity() {
  if (started) return;
  started = true;
  onCoreEvent((e) => {
    if (e.type === 'task') activity.value = reduceTask(activity.value, e);
  });
  try {
    const snap = await rpc<TaskEvent[]>('status.tasks');
    activity.value = mergeSnapshot(activity.value, snap ?? []);
  } catch {
    // Older core without the snapshot: live events still drive the light.
  }
}
