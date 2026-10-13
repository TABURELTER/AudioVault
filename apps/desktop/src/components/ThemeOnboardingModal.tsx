import { Moon, Sun, Check, Sparkles } from "lucide-react";

interface Props {
  isOpen: boolean;
  currentTheme: "dark" | "light";
  onSelectTheme: (theme: "dark" | "light") => void;
  onConfirm: () => void;
}

export function ThemeOnboardingModal({
  isOpen,
  currentTheme,
  onSelectTheme,
  onConfirm,
}: Props) {
  if (!isOpen) return null;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="theme-modal-title">
      <div className="modal" style={{ width: "min(540px, calc(100vw - 32px))", padding: "28px" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: "50%",
              background: "rgba(245, 158, 11, 0.16)",
              display: "grid",
              placeItems: "center",
              color: "var(--accent)",
            }}
          >
            <Sparkles size={20} />
          </div>
          <div>
            <h2 id="theme-modal-title" style={{ fontSize: 19, fontWeight: 700, color: "var(--text-primary)" }}>
              Добро пожаловать в AudioVault!
            </h2>
            <p style={{ margin: "2px 0 0", fontSize: 13, color: "var(--text-secondary)" }}>
              Выберите комфортное оформление для вашей библиотеки
            </p>
          </div>
        </div>

        <div className="theme-selector" style={{ marginTop: 22, marginBottom: 22 }}>
          {/* Dark Theme Card */}
          <div
            className={`theme-card ${currentTheme === "dark" ? "active" : ""}`}
            onClick={() => onSelectTheme("dark")}
            role="button"
            tabIndex={0}
            aria-pressed={currentTheme === "dark"}
          >
            <div
              className="theme-preview-box"
              style={{
                background: "#131316",
                padding: "8px",
                flexDirection: "column",
                justifyContent: "space-between",
                border: "1px solid #36364a",
              }}
            >
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <div style={{ width: 14, height: 14, borderRadius: 3, background: "#f59e0b" }} />
                <div style={{ height: 6, width: "60%", borderRadius: 2, background: "#2e2e3d" }} />
              </div>
              <div
                style={{
                  background: "#1c1c25",
                  border: "1px solid #36364a",
                  borderRadius: 4,
                  padding: "5px 6px",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div style={{ height: 6, width: "50%", borderRadius: 2, background: "#ffffff" }} />
                <div style={{ width: 12, height: 6, borderRadius: 2, background: "#10b981" }} />
              </div>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <Moon size={16} color="var(--accent)" />
              <strong>Тёмная тема</strong>
              {currentTheme === "dark" && <Check size={16} color="var(--accent)" />}
            </div>
            <p>
              Бархатный графит & античное золото. Идеально при вечернем чтении, не утомляет зрение.
            </p>
          </div>

          {/* Light Theme Card */}
          <div
            className={`theme-card ${currentTheme === "light" ? "active" : ""}`}
            onClick={() => onSelectTheme("light")}
            role="button"
            tabIndex={0}
            aria-pressed={currentTheme === "light"}
          >
            <div
              className="theme-preview-box"
              style={{
                background: "#f7f6f2",
                padding: "8px",
                flexDirection: "column",
                justifyContent: "space-between",
                border: "1px solid #d6cebf",
              }}
            >
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <div style={{ width: 14, height: 14, borderRadius: 3, background: "#d97706" }} />
                <div style={{ height: 6, width: "60%", borderRadius: 2, background: "#e2ddd4" }} />
              </div>
              <div
                style={{
                  background: "#ffffff",
                  border: "1px solid #d6cebf",
                  borderRadius: 4,
                  padding: "5px 6px",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div style={{ height: 6, width: "50%", borderRadius: 2, background: "#181512" }} />
                <div style={{ width: 12, height: 6, borderRadius: 2, background: "#059669" }} />
              </div>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <Sun size={16} color="var(--accent)" />
              <strong>Светлая тема</strong>
              {currentTheme === "light" && <Check size={16} color="var(--accent)" />}
            </div>
            <p>
              Эстетика книжной бумаги слоновой кости & тёплый янтарь. Мягкая и естественная.
            </p>
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 20 }}>
          <small style={{ color: "var(--text-muted)", fontSize: 12 }}>
            Вы сможете сменить тему в любой момент в «Настройках»
          </small>
          <button
            className="btn btn-primary"
            style={{
              height: 42,
              padding: "0 22px",
              fontSize: 14,
              fontWeight: 700,
              background: "var(--accent-gradient)",
              color: "var(--text-on-accent)",
              border: "none",
              borderRadius: "8px",
            }}
            onClick={onConfirm}
          >
            Применить и продолжить
          </button>
        </div>
      </div>
    </div>
  );
}
