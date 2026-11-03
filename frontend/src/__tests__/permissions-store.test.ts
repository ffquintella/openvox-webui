import { beforeEach, describe, expect, it } from 'vitest';
import { usePermissionsStore } from '../stores/permissionsStore';
import type { EffectivePermissions } from '../types';

function setPermissions(roles: string[], permissions: EffectivePermissions['permissions']) {
  usePermissionsStore.setState({
    permissions: { user_id: 'u1', roles, permissions },
  });
}

describe('permissionsStore.hasPermission', () => {
  beforeEach(() => usePermissionsStore.setState({ permissions: null }));

  it('denies everything before permissions are loaded', () => {
    expect(usePermissionsStore.getState().hasPermission('plugins', 'read')).toBe(false);
  });

  it('grants the super admin role every resource, including plugins', () => {
    setPermissions(['super_admin'], []);
    const { hasPermission } = usePermissionsStore.getState();
    expect(hasPermission('plugins', 'read')).toBe(true);
    expect(hasPermission('plugin_trust', 'admin')).toBe(true);
  });

  it('still requires an explicit permission for other roles', () => {
    setPermissions(['operator'], []);
    expect(usePermissionsStore.getState().hasPermission('plugins', 'read')).toBe(false);
    setPermissions(
      ['operator'],
      [{ resource: 'plugins', action: 'admin', scope: { type: 'all' } } as never]
    );
    expect(usePermissionsStore.getState().hasPermission('plugins', 'read')).toBe(true);
  });
});
