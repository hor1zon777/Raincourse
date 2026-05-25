import type { UserInfo } from '../types';

/**
 * 运行时校验 userinfo 接口返回。
 *
 * 雨课堂 /v2/api/web/userinfo 的返回格式在不同时期/不同账号下略有差异：
 * - `{ data: [{ user_id, name, school? }] }`（最常见）
 * - `{ data: { user_id, name, school? } }`（部分账号下是单对象）
 * - `user_id` 可能是 number 或 string
 * - 字段名也偶尔出现 `Name`/`School` 等驼峰/大写变体
 *
 * 任何字段无法解析则返回 null。
 */
export function parseUserInfo(resp: unknown): UserInfo | null {
  if (!resp || typeof resp !== 'object') return null;
  const root = resp as Record<string, unknown>;

  // 拆 data：array 或 单 object 都接受；如果根本没有 data，尝试把整个 resp 当 data
  let raw: unknown = root.data;
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
    raw = [raw];
  } else if (raw === undefined && typeof root.user_id !== 'undefined') {
    raw = [root];
  }
  if (!Array.isArray(raw) || raw.length === 0) return null;

  const first = raw[0];
  if (!first || typeof first !== 'object') return null;
  const obj = first as Record<string, unknown>;

  const rawId = obj.user_id ?? obj.userId ?? obj.UserID ?? obj.id;
  const rawName = obj.name ?? obj.Name ?? obj.username;
  const rawSchool = obj.school ?? obj.School;
  // 头像字段名雨课堂在不同接口/账号下有多种变体
  const rawAvatar =
    obj.head_image_url ??
    obj.headImageUrl ??
    obj.head_url ??
    obj.headUrl ??
    obj.avatar ??
    obj.Avatar ??
    obj.wx_head_img;

  // user_id: 接受 number 或可解析为整数的 string
  let userId: number | undefined;
  if (typeof rawId === 'number' && Number.isFinite(rawId)) {
    userId = rawId;
  } else if (typeof rawId === 'string') {
    const n = Number.parseInt(rawId.trim(), 10);
    if (Number.isFinite(n)) userId = n;
  }
  if (userId === undefined) return null;

  if (typeof rawName !== 'string' || rawName.length === 0) return null;

  return {
    user_id: userId,
    name: rawName,
    school: typeof rawSchool === 'string' ? rawSchool : undefined,
    avatar:
      typeof rawAvatar === 'string' && rawAvatar.length > 0 ? rawAvatar : undefined,
  };
}
