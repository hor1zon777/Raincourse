import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { UserInfo } from '../types';

interface AuthState {
  isLoggedIn: boolean;
  userInfo: UserInfo | null;
  savedUsers: string[];
  loading: boolean;
  error: string | null;

  initClient: () => Promise<void>;
  fetchSavedUsers: () => Promise<void>;
  loginWithSession: (username: string) => Promise<void>;
  startQrLogin: () => Promise<void>;
  logout: () => void;
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
      set({ error: String(e) });
    }
  },

  fetchSavedUsers: async () => {
    try {
      const users = await invoke<string[]>('get_saved_users');
      set({ savedUsers: users });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loginWithSession: async (username: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await invoke<Record<string, unknown>>('login_with_session', { username });
      const data = resp?.data as Record<string, unknown>[] | undefined;
      if (data && data.length > 0) {
        const user: UserInfo = {
          user_id: data[0].user_id as number,
          name: data[0].name as string,
          school: data[0].school as string | undefined,
        };
        set({ isLoggedIn: true, userInfo: user, loading: false });
      }
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  startQrLogin: async () => {
    set({ loading: true, error: null });
    try {
      await invoke('start_qr_login');
      // 登录成功后获取用户信息
      const resp = await invoke<Record<string, unknown>>('get_user_info');
      const data = resp?.data as Record<string, unknown>[] | undefined;
      if (data && data.length > 0) {
        const user: UserInfo = {
          user_id: data[0].user_id as number,
          name: data[0].name as string,
          school: data[0].school as string | undefined,
        };
        set({ isLoggedIn: true, userInfo: user, loading: false });
      }
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  logout: () => {
    set({ isLoggedIn: false, userInfo: null });
  },

  setUserInfo: (info: UserInfo) => {
    set({ isLoggedIn: true, userInfo: info });
  },
}));
