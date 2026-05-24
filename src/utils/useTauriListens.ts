import { useEffect, useRef } from 'react';
import { listen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';

export interface TauriSub {
  event: string;
  handler: EventCallback<unknown>;
}

/**
 * 安全订阅 Tauri 事件，处理 StrictMode 双调用 + 异步 cleanup 竞态。
 *
 * 直接写：
 * ```ts
 * useEffect(() => {
 *   const u = listen('foo', handler);
 *   return () => { u.then(fn => fn()); };
 * }, []);
 * ```
 * 在 React 18+ StrictMode 下，第一次 effect 的 cleanup 在 Promise resolve 前就被
 * 调用，`.then(fn => fn())` 注册的 unlisten 永远不会被执行 → 同一个监听器被注册两次。
 *
 * 本 hook 用 `cancelled` flag + Promise.all 串行注册，cleanup 时即使 Promise 仍在
 * pending 也能保证最终调用到 unlisten。
 *
 * `subs` 数组只在 mount 时使用一次，事件名固定；handlers 通过 ref 取最新闭包，
 * 不会因 handler 内部引用最新 state 而失效。
 */
export function useTauriListens(subs: TauriSub[]): void {
  // 用 ref 保存最新 handlers，effect 内调用时取最新引用
  const subsRef = useRef(subs);
  // React 19 规则禁止 render 时改 ref，用同步 effect 更新
  useEffect(() => {
    subsRef.current = subs;
  });

  useEffect(() => {
    let cancelled = false;
    let unlistenFns: UnlistenFn[] = [];

    // 事件名快照：mount 时确定，不会随后续重渲染改变
    const events = subsRef.current.map((s) => s.event);

    (async () => {
      try {
        const fns = await Promise.all(
          events.map((event, i) =>
            listen(event, (e) => {
              const current = subsRef.current[i];
              if (current) current.handler(e);
            }),
          ),
        );
        if (cancelled) {
          // effect 已被卸载：立即解绑
          fns.forEach((fn) => fn());
          return;
        }
        unlistenFns = fns;
      } catch (e) {
        console.error('useTauriListens 注册失败:', e);
      }
    })();

    return () => {
      cancelled = true;
      unlistenFns.forEach((fn) => fn());
    };
  }, []);
}
