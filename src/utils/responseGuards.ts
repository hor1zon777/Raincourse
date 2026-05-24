import type { UserInfo } from '../types';

/**
 * 运行时校验 userinfo 接口返回。
 * 后端返回 `{ data: [{ user_id, name, school? }] }`，任何字段缺失或类型错误都返回 null。
 */
export function parseUserInfo(resp: unknown): UserInfo | null {
  if (!resp || typeof resp !== 'object') return null;
  const data = (resp as Record<string, unknown>).data;
  if (!Array.isArray(data) || data.length === 0) return null;

  const first = data[0];
  if (!first || typeof first !== 'object') return null;

  const userId = (first as Record<string, unknown>).user_id;
  const name = (first as Record<string, unknown>).name;
  const school = (first as Record<string, unknown>).school;

  if (typeof userId !== 'number') return null;
  if (typeof name !== 'string' || name.length === 0) return null;

  return {
    user_id: userId,
    name,
    school: typeof school === 'string' ? school : undefined,
  };
}
