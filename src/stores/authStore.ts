import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { UserInfo } from '../types';
import { parseUserInfo } from '../utils/responseGuards';
import { normalizeError } from '../utils/errors';

interface AuthState {
  isLoggedIn: boolean;
  userInfo: UserInfo | null;
  savedUsers: string[];
  loading: boolean;
  error: string | null;

  initClient: () => Promise<void>;
  fetchSavedUsers: () => Promise<void>;
  loginWithSession: (username: string) => Promise<void>;
  removeSavedUser: (username: string) => Promise<void>;
  startQrLogin: () => Promise<void>;
  logout: () => Promise<void>;
  setUserInfo: (info: UserInfo) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  isLoggedIn: false,
  userInfo: null,
  savedUsers: [],
  loading: false,
  error: null,

  initClient: async () => {
    try {
      await invoke('init_client');
    } catch (e) {
      set({ error: normalizeError(e).message });
    }
  },

  fetchSavedUsers: async () => {
    try {
      const users = await invoke<string[]>('get_saved_users');
      set({ savedUsers: users });
    } catch (e) {
      set({ error: normalizeError(e).message });
    }
  },

  loginWithSession: async (username: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await invoke<unknown>('login_with_session', { username });
      const user = parseUserInfo(resp);
      if (user) {
        set({ isLoggedIn: true, userInfo: user, loading: false });
      } else {
        // 兜底：后端理论上应该返回 SESSION_EXPIRED，但响应格式异常时
        // 也防止 loading 永远不停
        set({ loading: false, isLoggedIn: false, userInfo: null });
        throw { code: 'SESSION_EXPIRED', message: '会话已过期，请重新扫码登录' };
      }
    } catch (e) {
      const err = normalizeError(e);
      set({
        loading: false,
        error: err.message,
        isLoggedIn: false,
        userInfo: null,
      });
      throw err;
    }
  },

  removeSavedUser: async (username: string) => {
    set({ loading: true, error: null });
    try {
      await invoke('remove_saved_user', { username });
      const users = await invoke<string[]>('get_saved_users');
      set((state) => ({
        savedUsers: users,
        isLoggedIn: state.userInfo?.name === username ? false : state.isLoggedIn,
        userInfo: state.userInfo?.name === username ? null : state.userInfo,
        loading: false,
      }));
    } catch (e) {
      const err = normalizeError(e);
      set({ loading: false, error: err.message });
      throw err;
    }
  },

  startQrLogin: async () => {
    set({ loading: true, error: null });
    try {
      await invoke('start_qr_login');
      const resp = await invoke<unknown>('get_user_info');
      const user = parseUserInfo(resp);
      if (user) {
        set({ isLoggedIn: true, userInfo: user, loading: false });
      } else {
        set({ loading: false });
        throw { code: 'GENERAL_ERROR', message: '扫码登录后未能获取用户信息，请重试' };
      }
    } catch (e) {
      const err = normalizeError(e);
      set({ loading: false, error: err.message });
      throw err;
    }
  },

  logout: async () => {
    // 通知后端清空 cookie jar，防止"切换用户"后旧 cookie 仍发请求
    try {
      await invoke('clear_session');
    } catch (e) {
      console.warn('clear_session 失败:', e);
    }
    set({ isLoggedIn: false, userInfo: null });
  },

  setUserInfo: (info: UserInfo) => {
    set({ isLoggedIn: true, userInfo: info });
  },
}));
