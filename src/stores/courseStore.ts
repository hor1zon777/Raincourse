import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Course, Work, Ppt } from '../types';
import { normalizeError } from '../utils/errors';

interface CourseState {
  courses: Course[];
  works: Work[];
  ppts: Ppt[];
  coursesLoading: boolean;
  worksLoading: boolean;
  pptsLoading: boolean;
  error: string | null;
  // 当前 works/ppts 对应的 courseId，切换课程时用来判断是否需要清空
  currentCourseId: string | null;

  fetchCourses: () => Promise<void>;
  fetchWorks: (courseId: string) => Promise<void>;
  fetchPpts: (courseId: string) => Promise<void>;
  clearWorks: () => void;
  setCourseContext: (courseId: string) => void;
}

export const useCourseStore = create<CourseState>((set, get) => ({
  courses: [],
  works: [],
  ppts: [],
  coursesLoading: false,
  worksLoading: false,
  pptsLoading: false,
  error: null,
  currentCourseId: null,

  fetchCourses: async () => {
    set({ coursesLoading: true, error: null });
    try {
      const courses = await invoke<Course[]>('get_course_list');
      set({ courses, coursesLoading: false });
    } catch (e) {
      set({ coursesLoading: false, error: normalizeError(e).message });
    }
  },

  fetchWorks: async (courseId: string) => {
    set({ worksLoading: true, error: null });
    try {
      const works = await invoke<Work[]>('get_course_works', { courseId });
      // 异步竞争：如果 await 期间课程已切换，不要写回旧数据
      if (get().currentCourseId !== courseId) {
        set({ worksLoading: false });
        return;
      }
      set({ works, worksLoading: false });
    } catch (e) {
      set({ worksLoading: false, error: normalizeError(e).message });
    }
  },

  fetchPpts: async (courseId: string) => {
    set({ pptsLoading: true, error: null });
    try {
      const ppts = await invoke<Ppt[]>('get_course_ppts', { courseId });
      if (get().currentCourseId !== courseId) {
        set({ pptsLoading: false });
        return;
      }
      set({ ppts, pptsLoading: false });
    } catch (e) {
      set({ pptsLoading: false, error: normalizeError(e).message });
    }
  },

  clearWorks: () => {
    set({ works: [], ppts: [] });
  },

  /**
   * 切换到新课程：清空旧数据，记录新 courseId。
   * 必须在 fetchWorks/fetchPpts 之前调用。
   */
  setCourseContext: (courseId: string) => {
    const prev = get().currentCourseId;
    if (prev !== courseId) {
      set({ works: [], ppts: [], currentCourseId: courseId });
    }
  },
}));
