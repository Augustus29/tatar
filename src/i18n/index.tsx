import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import tr from './tr.json';
import en from './en.json';
import { broadcast, onEvent } from '../lib/tauri';

export type Lang = 'tr' | 'en';
export type LangPref = 'system' | Lang;

const dictionaries: Record<Lang, unknown> = { tr, en };
const STORAGE_KEY = 'tatar.lang';
const EVENT = 'prefs-language-changed';

function systemLang(): Lang {
  return navigator.language.toLowerCase().startsWith('tr') ? 'tr' : 'en';
}

function readPref(): LangPref {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === 'tr' || v === 'en' || v === 'system') return v;
  } catch {
    /* localStorage kullanılamıyorsa varsayılan */
  }
  return 'system';
}

function lookup(dict: unknown, key: string): string | undefined {
  let cur: unknown = dict;
  for (const part of key.split('.')) {
    if (typeof cur !== 'object' || cur === null) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

export type TFunction = (key: string, params?: Record<string, string | number>) => string;

interface I18nValue {
  t: TFunction;
  lang: Lang;
  pref: LangPref;
  setPref: (p: LangPref) => void;
  /** Dile uygun sayı biçimi, ör. 4,2 MB / 4.2 MB */
  formatSize: (bytes: number) => string;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [pref, setPrefState] = useState<LangPref>(readPref);
  const lang: Lang = pref === 'system' ? systemLang() : pref;

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  // Diğer pencerede (ana pencere ↔ popover) dil değişince burada da değişsin.
  useEffect(() => {
    const sync = () => setPrefState(readPref());
    const off = onEvent(EVENT, sync);
    window.addEventListener('storage', sync);
    return () => {
      off();
      window.removeEventListener('storage', sync);
    };
  }, []);

  const setPref = useCallback((p: LangPref) => {
    try {
      localStorage.setItem(STORAGE_KEY, p);
    } catch {
      /* yok say */
    }
    setPrefState(p);
    broadcast(EVENT);
  }, []);

  const value = useMemo<I18nValue>(() => {
    const dict = dictionaries[lang];
    const t: TFunction = (key, params) => {
      const raw = lookup(dict, key) ?? lookup(dictionaries.tr, key);
      if (raw === undefined) {
        console.warn(`eksik çeviri: ${key}`);
        return key;
      }
      return params ? raw.replace(/\{(\w+)\}/g, (_, k: string) => String(params[k] ?? `{${k}}`)) : raw;
    };
    const formatSize = (b: number) => {
      const nf = (n: number) => n.toLocaleString(lang === 'tr' ? 'tr-TR' : 'en-US', { maximumFractionDigits: 1, minimumFractionDigits: 1 });
      if (b >= 1048576) return `${nf(b / 1048576)} MB`;
      return `${Math.max(1, Math.round(b / 1024))} KB`;
    };
    return { t, lang, pref, setPref, formatSize };
  }, [lang, pref, setPref]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const v = useContext(I18nContext);
  if (!v) throw new Error('I18nProvider eksik');
  return v;
}
