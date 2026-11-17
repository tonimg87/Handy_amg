// Fork amg: per-app rules for the plain transcribe shortcut.
import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUp, RefreshCcw, X } from "lucide-react";
import { commands, type AppRule, type ForegroundApp } from "@/bindings";
import { Dropdown, SettingContainer, ToggleSwitch } from "@/components/ui";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { ResetButton } from "../../ui/ResetButton";
import { useSettings } from "../../../hooks/useSettings";

const RAW = "__raw__";

const newRuleId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `rule-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

export const AppRulesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const enabled = getSetting("app_rules_enabled") || false;
  const savedRules = getSetting("app_rules") || [];
  const defaultPromptId = getSetting("app_rules_default_prompt_id") ?? null;
  const prompts = getSetting("post_process_prompts") || [];

  const [rules, setRules] = useState<AppRule[]>(savedRules);
  const [lastApp, setLastApp] = useState<ForegroundApp | null>(null);

  // Resync when the stored rules change elsewhere.
  const savedKey = JSON.stringify(savedRules);
  useEffect(() => {
    setRules(JSON.parse(savedKey) as AppRule[]);
  }, [savedKey]);

  const refreshLastApp = useCallback(async () => {
    try {
      setLastApp(await commands.getLastTargetApp());
    } catch (error) {
      console.error("Failed to read last target app:", error);
    }
  }, []);

  useEffect(() => {
    refreshLastApp();
  }, [refreshLastApp]);

  const promptOptions = [
    { value: RAW, label: t("settings.postProcessing.appRules.raw") },
    ...prompts.map((p) => ({ value: p.id, label: p.name })),
  ];

  const save = (next: AppRule[]) => {
    setRules(next);
    updateSetting("app_rules", next);
  };

  const editLocal = (id: string, patch: Partial<AppRule>) =>
    setRules((current) =>
      current.map((rule) => (rule.id === id ? { ...rule, ...patch } : rule)),
    );

  const commitLocal = () => {
    if (JSON.stringify(rules) !== JSON.stringify(savedRules)) {
      updateSetting("app_rules", rules);
    }
  };

  const addRule = (processName = "", titleContains = "") =>
    save([
      ...rules,
      {
        id: newRuleId(),
        process_name: processName,
        title_contains: titleContains,
        prompt_id: null,
      },
    ]);

  const removeRule = (id: string) =>
    save(rules.filter((rule) => rule.id !== id));

  const moveRule = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    const next = [...rules];
    [next[index], next[target]] = [next[target], next[index]];
    save(next);
  };

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("app_rules_enabled", value)}
        isUpdating={isUpdating("app_rules_enabled")}
        label={t("settings.postProcessing.appRules.enabled.title")}
        description={t("settings.postProcessing.appRules.enabled.description")}
        descriptionMode="tooltip"
        grouped={true}
      />

      <SettingContainer
        title={t("settings.postProcessing.appRules.default.title")}
        description={t("settings.postProcessing.appRules.default.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
        disabled={!enabled}
      >
        <Dropdown
          options={promptOptions}
          selectedValue={defaultPromptId ?? RAW}
          onSelect={(value) =>
            updateSetting(
              "app_rules_default_prompt_id",
              value === RAW ? null : value,
            )
          }
          disabled={!enabled || isUpdating("app_rules_default_prompt_id")}
          className="min-w-[220px]"
        />
      </SettingContainer>

      <SettingContainer
        title={t("settings.postProcessing.appRules.list.title")}
        description={t("settings.postProcessing.appRules.list.description")}
        descriptionMode="tooltip"
        layout="stacked"
        grouped={true}
        disabled={!enabled}
      >
        <div className="space-y-2">
          {rules.length === 0 && (
            <p className="text-sm text-ink-3">
              {t("settings.postProcessing.appRules.list.empty")}
            </p>
          )}

          {rules.map((rule, index) => (
            <div key={rule.id} className="flex items-center gap-2 min-w-0">
              <Input
                type="text"
                variant="compact"
                className="w-36"
                value={rule.process_name ?? ""}
                placeholder={t(
                  "settings.postProcessing.appRules.list.processPlaceholder",
                )}
                onChange={(e) =>
                  editLocal(rule.id, { process_name: e.target.value })
                }
                onBlur={commitLocal}
                disabled={!enabled}
              />
              <Input
                type="text"
                variant="compact"
                className="w-36"
                value={rule.title_contains ?? ""}
                placeholder={t(
                  "settings.postProcessing.appRules.list.titlePlaceholder",
                )}
                onChange={(e) =>
                  editLocal(rule.id, { title_contains: e.target.value })
                }
                onBlur={commitLocal}
                disabled={!enabled}
              />
              <Dropdown
                options={promptOptions}
                selectedValue={rule.prompt_id ?? RAW}
                onSelect={(value) =>
                  save(
                    rules.map((r) =>
                      r.id === rule.id
                        ? { ...r, prompt_id: value === RAW ? null : value }
                        : r,
                    ),
                  )
                }
                disabled={!enabled}
                className="flex-1 min-w-0"
              />
              <Button
                variant="secondary"
                size="sm"
                onClick={() => moveRule(index, -1)}
                disabled={!enabled || index === 0}
                aria-label={t("settings.postProcessing.appRules.list.moveUp")}
              >
                <ArrowUp className="h-3 w-3" />
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => removeRule(rule.id)}
                disabled={!enabled}
                aria-label={t("settings.postProcessing.appRules.list.remove")}
              >
                <X className="h-3 w-3" />
              </Button>
            </div>
          ))}

          <div className="flex items-center gap-2 pt-1">
            <Button
              variant="primary"
              size="md"
              onClick={() => addRule()}
              disabled={!enabled}
            >
              {t("settings.postProcessing.appRules.list.add")}
            </Button>
          </div>

          <div className="flex items-center gap-2 pt-2 text-sm text-ink-3 min-w-0">
            <span className="truncate">
              {lastApp
                ? t("settings.postProcessing.appRules.lastApp.value", {
                    process: lastApp.process_name,
                    title: lastApp.window_title,
                  })
                : t("settings.postProcessing.appRules.lastApp.none")}
            </span>
            <ResetButton
              onClick={refreshLastApp}
              ariaLabel={t("settings.postProcessing.appRules.lastApp.refresh")}
              className="flex h-8 w-8 items-center justify-center shrink-0"
            >
              <RefreshCcw className="h-4 w-4" />
            </ResetButton>
            {lastApp && (
              <Button
                variant="secondary"
                size="sm"
                className="shrink-0"
                onClick={() => addRule(lastApp.process_name)}
                disabled={!enabled}
              >
                {t("settings.postProcessing.appRules.lastApp.use")}
              </Button>
            )}
          </div>
        </div>
      </SettingContainer>
    </>
  );
};
