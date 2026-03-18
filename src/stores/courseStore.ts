import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Course, Work, Ppt } from '../types';

interface CourseState {
  courses: Course[];
  works: Work[];
  ppts: Ppt[];
  loading: boolean;
  error: string | null;

  fetchCourses: () => Promise<void>;
  fetchWorks: (courseId: string) => Promise<void>;
  fetchPpts: (courseId: string) => Promise<void>;
  clearWorks: () => void;
}

export const useCourseStore = create<CourseState>((set) => ({
  courses: [],
  works: [],
  ppts: [],
  loading: false,
  error: null,

  fetchCourses: async () => {
    set({ loading: true, error: null });
    try {
      const courses = await invoke<Course[]>('get_course_list');
      set({ courses, loading: false });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  fetchWorks: async (courseId: string) => {
    set({ loading: true, error: null });
    try {
      const works = await invoke<Work[]>('get_course_works', { courseId });
      set({ works, loading: false });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  fetchPpts: async (courseId: string) => {
    set({ loading: true, error: null });
    try {
      const ppts = await invoke<Ppt[]>('get_course_ppts', { courseId });
      set({ ppts, loading: false });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  clearWorks: () => {
    set({ works: [], ppts: [] });
  },
}));
