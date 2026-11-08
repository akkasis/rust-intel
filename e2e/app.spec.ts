import { expect, test } from "@playwright/test";

const DEMO_ID = "76561202060265729";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  const demo = page.getByRole("button", { name: /Демо-режим/ });
  const overview = page.getByRole("heading", { name: "Обзор" });
  await expect(demo.or(overview)).toBeVisible();
  if (await demo.isVisible()) await demo.click();
  await expect(overview).toBeVisible();
});

test("демо-лента и обзор", async ({ page }) => {
  await expect(page.getByText("Все игроки, серверы и события вымышлены")).toBeVisible();
  await expect(page.getByText("Последние события")).toBeVisible();
  await expect(page.getByText("Состояние источников")).toBeVisible();
});

test("поиск по SteamID64 открывает профиль, ошибки валидации видны", async ({ page }) => {
  await page.getByRole("link", { name: "Поиск игрока" }).click();
  const input = page.getByLabel("Идентификатор игрока");
  await input.fill("123");
  await expect(page.getByText(/ожидается 17 цифр/)).toBeVisible();
  await input.fill(DEMO_ID);
  await page.getByRole("button", { name: "Найти" }).click();
  await expect(page.getByRole("heading", { name: "DEMO·Ворон" })).toBeVisible();
  await expect(page.getByText("STEAM_0:1:2050000000")).toBeVisible();
});

test("активность: тепловая карта 7×24 и неизвестные ячейки", async ({ page }) => {
  await page.goto(`/#/players/${DEMO_ID}`);
  await page.getByRole("tab", { name: "Активность" }).click();
  await expect(page.getByRole("grid", { name: /Активность по дням недели/ })).toBeVisible();
  await expect(page.getByRole("gridcell")).toHaveCount(168);
  await page.getByRole("radio", { name: "90 дней" }).click();
  await expect(page.getByText("Покрытие", { exact: true })).toBeVisible();
});

test("недостаточно данных — нет «прогноза сна»", async ({ page }) => {
  await page.goto(`/#/players/76561202060265734`);
  await page.getByRole("tab", { name: "Активность" }).click();
  await page.getByRole("radio", { name: "90 дней" }).click();
  await expect(page.getByText(/Недостаточно данных/)).toBeVisible();
});

test("отряды и трекер", async ({ page }) => {
  await page.getByRole("link", { name: "Отряды" }).click();
  await expect(page.getByText("Совместная временная шкала")).toBeVisible();
  await page.getByRole("link", { name: "Live-трекер" }).click();
  await expect(page.getByText("Журнал событий")).toBeVisible();
});

test("командная палитра Ctrl+K", async ({ page }) => {
  await page.keyboard.press("Control+k");
  await page.getByLabel("Запрос").fill("Клещ");
  await expect(page.getByRole("option").first()).toContainText("DEMO·Клещ");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "DEMO·Клещ" })).toBeVisible();
});

test("инструкция открывается и ищется", async ({ page }) => {
  await page.getByRole("link", { name: "Инструкция" }).first().click();
  await expect(page.getByRole("heading", { name: "Быстрый старт" })).toBeVisible();
  await page.getByLabel("Поиск по инструкции").fill("telegram");
  await expect(page.getByRole("heading", { name: "Уведомления Windows и Telegram" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Быстрый старт" })).toHaveCount(0);
});

test("BattleMetrics: игрок, тиммейты, онлайн сервера", async ({ page }) => {
  await page.getByRole("link", { name: "BattleMetrics" }).click();
  await page.getByLabel("Фильтр игроков").fill("Ворон");
  await page.getByRole("link", { name: /DEMO·Ворон/ }).first().click();
  await expect(page.getByRole("heading", { name: "DEMO·Ворон" })).toBeVisible();
  await page.getByRole("tab", { name: /Тиммейты/ }).click();
  await expect(page.getByRole("link", { name: "DEMO·Сосед" })).toBeVisible();
  await page.getByRole("tab", { name: /Сессии/ }).click();
  await expect(page.getByText("Зашёл")).toBeVisible();
  await page.goto("/#/servers?id=demo%3A1");
  await expect(page.getByText("Онлайн сейчас")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Наблюдать за сервером" })).toBeChecked();
});

test("инсайты: распорядок, после вайпа, новый аккаунт, клан, топ сервера", async ({ page }) => {
  await page.goto("/#/bm/9100001");
  await page.getByRole("tab", { name: "Распорядок" }).click();
  await expect(page.getByText("Прогноз на 24 часа")).toBeVisible();
  await page.getByRole("tab", { name: "После вайпа" }).click();
  await expect(page.getByText("Сколько играет по дням после вайпа")).toBeVisible();
  await page.getByRole("tab", { name: "Новый аккаунт?" }).click();
  await expect(page.getByRole("link", { name: "DEMO·Новый_акк" })).toBeVisible();
  await page.getByRole("link", { name: "Кланы" }).click();
  await expect(page.getByRole("heading", { name: "[DEMO] Северяне (BM)" })).toBeVisible();
  await expect(page.getByText("Совместная шкала")).toBeVisible();
  await page.goto("/#/servers?id=demo%3A1");
  await expect(page.getByText("Топ по времени")).toBeVisible();
});

test("связи: друзья Steam и тиммейты без случайных игроков", async ({ page }) => {
  await page.goto("/#/players/76561202060265729");
  await page.getByRole("tab", { name: /Связи/ }).click();
  await expect(page.getByRole("link", { name: "DEMO·Клещ" })).toBeVisible();
  await expect(page.getByText("друг Steam").first()).toBeVisible();
  await page.getByRole("radio", { name: /Друзья Steam/ }).click();
  await expect(page.getByText("DEMO·Новичок")).toBeVisible();
});
