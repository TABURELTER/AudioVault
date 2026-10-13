import React, { useState, useEffect } from 'react';
import {
  Key,
  Shield,
  FolderOpen,
  Check,
  HardDrive,
  Cpu,
  Palette,
  Eye,
  EyeOff,
} from 'lucide-react';
import { api } from '../api/client';
import type { Settings } from '../api/types';

export const SettingsPage: React.FC = () => {
  const [settings, setSettings] = useState<Settings>({
    rutracker_username: '',
    ffmpeg_path: '',
    ffmpeg_version: '',
    ffmpeg_ok: false,
    theme: 'dark',
    db_path: '',
    logs_path: '',
  });

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [savingCreds, setSavingCreds] = useState(false);
  const [credsSaved, setCredsSaved] = useState(false);

  const [customFfmpeg, setCustomFfmpeg] = useState('');
  const [savingFfmpeg, setSavingFfmpeg] = useState(false);
  const [ffmpegSaved, setFfmpegSaved] = useState(false);

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      const data = await api.getSettings();
      setSettings(data);
      setUsername(data.rutracker_username || '');
      setCustomFfmpeg(data.ffmpeg_path || '');
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  };

  const handleSaveCredentials = async () => {
    if (!username.trim()) return;
    setSavingCreds(true);
    try {
      const ok = await api.saveCredentials(username, password);
      if (ok) {
        setCredsSaved(true);
        setPassword('');
        setTimeout(() => setCredsSaved(false), 3000);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setSavingCreds(false);
    }
  };

  const handleSaveFfmpeg = async () => {
    setSavingFfmpeg(true);
    try {
      const ok = await api.saveSettings({ ffmpeg_path: customFfmpeg });
      if (ok) {
        setFfmpegSaved(true);
        await loadSettings();
        setTimeout(() => setFfmpegSaved(false), 3000);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setSavingFfmpeg(false);
    }
  };

  const handleReveal = async (path: string) => {
    if (!path) return;
    try {
      await api.revealFolder(path);
    } catch (e) {
      console.error('Failed to reveal folder:', e);
    }
  };

  return (
    <div style={{ padding: '32px', maxWidth: '1000px', margin: '0 auto' }}>
      {/* Page Header */}
      <div style={{ marginBottom: '32px' }}>
        <h1
          style={{
            fontSize: '26px',
            fontWeight: 800,
            letterSpacing: '-0.02em',
            marginBottom: '6px',
          }}
        >
          Настройки Приложения
        </h1>
        <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
          Управление интеграциями, внешними утилитами и системными каталогами хранения.
        </p>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
        {/* Section 1: RuTracker Account */}
        <div className="glass-card" style={{ padding: '24px 28px' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              marginBottom: '16px',
            }}
          >
            <div
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '10px',
                background: 'rgba(59, 130, 246, 0.15)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Key size={18} color="#60a5fa" />
            </div>
            <div>
              <h2 style={{ fontSize: '17px', fontWeight: 700 }}>
                Учётная запись RuTracker
              </h2>
              <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
                Используется для авторизации и скачивания .torrent файлов с трекера
              </p>
            </div>
          </div>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '16px',
              marginBottom: '16px',
            }}
          >
            <div>
              <label
                style={{
                  display: 'block',
                  fontSize: '12.5px',
                  fontWeight: 600,
                  color: 'var(--text-secondary)',
                  marginBottom: '6px',
                }}
              >
                Имя пользователя (Логин)
              </label>
              <input
                type="text"
                placeholder="Логин на rutracker.org"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="input-text"
              />
            </div>

            <div>
              <label
                style={{
                  display: 'block',
                  fontSize: '12.5px',
                  fontWeight: 600,
                  color: 'var(--text-secondary)',
                  marginBottom: '6px',
                }}
              >
                Пароль
              </label>
              <div style={{ position: 'relative' }}>
                <input
                  type={showPassword ? 'text' : 'password'}
                  placeholder={settings.rutracker_username ? '••••••••••••' : 'Пароль'}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="input-text"
                  style={{ paddingRight: '40px' }}
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  style={{
                    position: 'absolute',
                    right: '12px',
                    top: '11px',
                    background: 'transparent',
                    border: 'none',
                    color: 'var(--text-muted)',
                    cursor: 'pointer',
                  }}
                >
                  {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              </div>
            </div>
          </div>

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              paddingTop: '8px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', color: 'var(--text-muted)', fontSize: '12.5px' }}>
              <Shield size={14} color="var(--success)" />
              <span>
                Пароли шифруются и безопасно хранятся в системном хранилище Windows Credential Manager.
              </span>
            </div>

            <button
              onClick={handleSaveCredentials}
              disabled={savingCreds || !username.trim()}
              className="btn btn-primary"
              style={{ padding: '8px 20px', fontSize: '13px' }}
            >
              {credsSaved ? (
                <>
                  <Check size={14} />
                  <span>Сохранено!</span>
                </>
              ) : (
                <span>Сохранить пароль</span>
              )}
            </button>
          </div>
        </div>

        {/* Section 2: FFmpeg Integration */}
        <div className="glass-card" style={{ padding: '24px 28px' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              marginBottom: '16px',
            }}
          >
            <div
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '10px',
                background: 'rgba(168, 85, 247, 0.15)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Cpu size={18} color="#c084fc" />
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                <h2 style={{ fontSize: '17px', fontWeight: 700 }}>
                  Медиа-движок FFmpeg
                </h2>
                <span
                  style={{
                    fontSize: '11px',
                    fontWeight: 700,
                    padding: '2px 8px',
                    borderRadius: '4px',
                    background: settings.ffmpeg_ok
                      ? 'rgba(16, 185, 129, 0.15)'
                      : 'rgba(239, 68, 68, 0.15)',
                    color: settings.ffmpeg_ok
                      ? 'var(--success)'
                      : 'var(--danger)',
                    border: `1px solid ${settings.ffmpeg_ok ? 'rgba(16, 185, 129, 0.3)' : 'rgba(239, 68, 68, 0.3)'}`,
                  }}
                >
                  {settings.ffmpeg_ok ? 'Обнаружен' : 'Не найден'}
                </span>
              </div>
              <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
                {settings.ffmpeg_version || 'Утилита для быстрого извлечения метаданных и конвертации в M4B'}
              </p>
            </div>
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label
              style={{
                display: 'block',
                fontSize: '12.5px',
                fontWeight: 600,
                color: 'var(--text-secondary)',
                marginBottom: '6px',
              }}
            >
              Путь к исполняемому файлу ffmpeg.exe (необязательно, если ffmpeg есть в PATH):
            </label>
            <div style={{ display: 'flex', gap: '12px' }}>
              <input
                type="text"
                placeholder="C:\ffmpeg\bin\ffmpeg.exe или ffmpeg"
                value={customFfmpeg}
                onChange={(e) => setCustomFfmpeg(e.target.value)}
                className="input-text"
                style={{ flex: 1 }}
              />
              <button
                onClick={handleSaveFfmpeg}
                disabled={savingFfmpeg}
                className="btn btn-secondary"
                style={{ padding: '0 20px', fontSize: '13px' }}
              >
                {ffmpegSaved ? 'Сохранено' : 'Проверить и сохранить'}
              </button>
            </div>
          </div>
        </div>

        {/* Section 3: Database & File Storage */}
        <div className="glass-card" style={{ padding: '24px 28px' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              marginBottom: '16px',
            }}
          >
            <div
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '10px',
                background: 'rgba(16, 185, 129, 0.15)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <HardDrive size={18} color="var(--success)" />
            </div>
            <div>
              <h2 style={{ fontSize: '17px', fontWeight: 700 }}>
                База Данных и Логи
              </h2>
              <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
                Расположение локального каталога SQLite и диагностических журналов
              </p>
            </div>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
            {/* DB Path */}
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '12px 16px',
                borderRadius: '8px',
                background: 'rgba(0, 0, 0, 0.25)',
                border: '1px solid var(--border-subtle)',
              }}
            >
              <div style={{ minWidth: 0, flex: 1, paddingRight: '16px' }}>
                <span style={{ fontSize: '12px', color: 'var(--text-muted)', display: 'block' }}>
                  База данных (SQLite + FTS5):
                </span>
                <span
                  style={{
                    fontSize: '13px',
                    fontFamily: 'monospace',
                    color: 'var(--text-primary)',
                    wordBreak: 'break-all',
                  }}
                >
                  {settings.db_path || 'По умолчанию в AppData'}
                </span>
              </div>
              <button
                onClick={() => handleReveal(settings.db_path)}
                className="btn btn-secondary"
                style={{ padding: '6px 12px', fontSize: '12.5px', flexShrink: 0 }}
                title="Открыть в проводнике"
              >
                <FolderOpen size={14} />
                <span>Открыть папку</span>
              </button>
            </div>

            {/* Logs Path */}
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '12px 16px',
                borderRadius: '8px',
                background: 'rgba(0, 0, 0, 0.25)',
                border: '1px solid var(--border-subtle)',
              }}
            >
              <div style={{ minWidth: 0, flex: 1, paddingRight: '16px' }}>
                <span style={{ fontSize: '12px', color: 'var(--text-muted)', display: 'block' }}>
                  Журналы работы (Logs):
                </span>
                <span
                  style={{
                    fontSize: '13px',
                    fontFamily: 'monospace',
                    color: 'var(--text-primary)',
                    wordBreak: 'break-all',
                  }}
                >
                  {settings.logs_path || 'По умолчанию в AppData/logs'}
                </span>
              </div>
              <button
                onClick={() => handleReveal(settings.logs_path)}
                className="btn btn-secondary"
                style={{ padding: '6px 12px', fontSize: '12.5px', flexShrink: 0 }}
                title="Открыть в проводнике"
              >
                <FolderOpen size={14} />
                <span>Открыть папку</span>
              </button>
            </div>
          </div>
        </div>

        {/* Section 4: Design & Aesthetics */}
        <div className="glass-card" style={{ padding: '24px 28px' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              marginBottom: '12px',
            }}
          >
            <div
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '10px',
                background: 'rgba(236, 72, 153, 0.15)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Palette size={18} color="#f472b6" />
            </div>
            <div>
              <h2 style={{ fontSize: '17px', fontWeight: 700 }}>
                Интерфейс
              </h2>
              <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
                Тема оформления и внешний вид
              </p>
            </div>
          </div>

          <p style={{ fontSize: '13.5px', color: 'var(--text-secondary)' }}>
            Активна премиальная тёмная тема <strong>Obsidian Glassmorphism</strong> с плавной аппаратной акселерацией WebKit/WebView2, поддержкой шрифтов Inter и JetBrains Mono.
          </p>
        </div>
      </div>
    </div>
  );
};
