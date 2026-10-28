import {
  Activity,
  BarChart3,
  Bell,
  Boxes,
  Cloud,
  Cpu,
  Database,
  FileText,
  Globe,
  KeyRound,
  Link,
  Plug,
  Puzzle,
  Server,
  Settings,
  Shield,
  type LucideIcon,
} from 'lucide-react';

/**
 * Icons a plugin may request by name. Anything outside this list falls back to
 * the Puzzle icon, so a manifest can never pull arbitrary components in.
 */
export const PLUGIN_ICON_ALLOWLIST: Readonly<Record<string, LucideIcon>> = {
  KeyRound,
  Puzzle,
  Boxes,
  Database,
  FileText,
  Globe,
  Link,
  Shield,
  Server,
  Settings,
  BarChart3,
  Bell,
  Cloud,
  Cpu,
  Activity,
  Plug,
};

export function resolvePluginIcon(name: string | null | undefined): LucideIcon {
  if (name && Object.prototype.hasOwnProperty.call(PLUGIN_ICON_ALLOWLIST, name)) {
    return PLUGIN_ICON_ALLOWLIST[name];
  }
  return Puzzle;
}
