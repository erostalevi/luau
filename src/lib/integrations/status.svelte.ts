// Global integration status (connectivity, sync activity) for the top-bar status light.

export const integrationStatus = $state({
  /** Non-empty when an account is unreachable ("Jira offline · last synced 10:42"). */
  offline: '' as string,
  syncing: 0,
});
