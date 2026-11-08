import { useMemo, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { BookOpen, ExternalLink, Search } from "lucide-react";
import { openExternal } from "@/lib/external";
import { Card, Input, Notice, PageHeader, cn } from "@/components/ui";

interface Section {
  id: string;
  title: string;
  keywords: string;
  body: ReactNode;
}

function Ext({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button type="button" onClick={() => openExternal(href)} className="inline-flex items-center gap-0.5 text-teal hover:underline">
      {children}
      <ExternalLink className="size-3" />
    </button>
  );
}

function To({ to, children }: { to: string; children: ReactNode }) {
  return (
    <Link to={to} className="text-teal hover:underline">
      {children}
    </Link>
  );
}

function Steps({ items }: { items: ReactNode[] }) {
  return (
    <ol className="space-y-1.5">
      {items.map((it, i) => (
        <li key={i} className="flex gap-2.5">
          <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-full bg-teal/15 text-[11px] font-semibold text-teal">{i + 1}</span>
          <span>{it}</span>
        </li>
      ))}
    </ol>
  );
}

function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="rounded border border-line bg-raised px-1 font-mono text-[11px]">{children}</kbd>;
}

const SECTIONS: Section[] = [
  {
    id: "start",
    title: "Быстрый старт",
    keywords: "начало запуск первый режим демо live",
    body: (
      <>
        <p>RID работает целиком на вашем компьютере: без регистрации, аккаунтов и облака. Данные лежат в <code>%APPDATA%\dev.rid.desktop</code>.</p>
        <Steps
          items={[
            <>При первом запуске выберите <b>Демо</b>, чтобы посмотреть все экраны на вымышленных игроках (у них имена «DEMO·…»).</>,
            <>Когда освоитесь — <To to="/settings">Настройки</To> → «Режим работы» → <b>Live</b>. Демо и Live хранятся в разных базах и не смешиваются.</>,
            <>В <To to="/integrations">Интеграциях</To> вставьте бесплатный Steam Web API key (как получить — ниже). Это минимум для реальных данных.</>,
            <>Откройте <To to="/search">Поиск игрока</To>, вставьте ссылку на профиль Steam — откроется карточка игрока.</>,
            <>В карточке включите переключатель <b>Live-трекер</b>, чтобы программа начала следить, когда игрок в Rust.</>,
          ]}
        />
      </>
    ),
  },
  {
    id: "keys",
    title: "Ключи: где взять и куда вставить",
    keywords: "ключ api key steam battlemetrics токен интеграции",
    body: (
      <>
        <h3 className="font-semibold">Steam Web API key — бесплатно, нужен обязательно</h3>
        <Steps
          items={[
            <>Откройте <Ext href="https://steamcommunity.com/dev/apikey">steamcommunity.com/dev/apikey</Ext> и войдите в Steam.</>,
            <>В поле «Domain Name» впишите что угодно, например <code>localhost</code>, согласитесь с условиями, нажмите «Register».</>,
            <>Скопируйте ключ (32 символа) → <To to="/integrations">Интеграции</To> → «Steam Web API» → вставить → «Сохранить» → «Проверить подключение».</>,
          ]}
        />
        <p className="text-muted">Steam выдаёт ключ только аккаунтам, на которых были покупки (от $5). Лимит — 100 000 запросов в сутки, этого с запасом хватает.</p>
        <h3 className="mt-3 font-semibold">battlemetrics.ru — бесплатно, по желанию</h3>
        <p>
          Даёт список серверов Rust, их онлайн и даты вайпов. Ключ вида <code>bm_live_…</code> на <Ext href="https://battlemetrics.ru/api">battlemetrics.ru/api</Ext>. Вставляется так же в «Интеграциях».
          Информации об отдельных игроках этот источник не даёт.
        </p>
        <h3 className="mt-3 font-semibold">BattleMetrics.com — платно, по желанию</h3>
        <p>
          Единственный источник истории заходов игрока на конкретные серверы. Токен создаётся в настройках разработчика BattleMetrics, но API работает только с платной подпиской — без неё будет статус
          «доступ запрещён».
        </p>
        <Notice tone="info" className="mt-3">
          Ключи хранятся в Windows Credential Manager, в интерфейс и файлы экспорта не попадают. Вводить их нужно один раз.
        </Notice>
      </>
    ),
  },
  {
    id: "search",
    title: "Как найти игрока",
    keywords: "поиск steamid ссылка vanity ник steam2 steam3",
    body: (
      <>
        <p>
          В <To to="/search">Поиск игрока</To> или через <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> можно вставить:
        </p>
        <ul className="ml-4 list-disc space-y-0.5">
          <li>ссылку на профиль: <code>steamcommunity.com/profiles/7656119…</code> или <code>steamcommunity.com/id/имя</code>;</li>
          <li>SteamID64 — 17 цифр, начинается с 7656119;</li>
          <li>форматы <code>STEAM_0:1:12345</code> и <code>[U:1:24691]</code> (из консоли Rust/серверных логов).</li>
        </ul>
        <p className="mt-2">
          Где взять ссылку: Steam → профиль игрока → правый клик → «Скопировать адрес страницы». В игре: Tab → игрок → открыть профиль Steam.
        </p>
        <p className="mt-2 text-muted">
          Поиск по нику Steam не поддерживает — только по уже найденным игрокам в вашей базе или через BattleMetrics.com. Совпадения по нику никогда не выбираются автоматически: ник легко подделать.
        </p>
      </>
    ),
  },
  {
    id: "profile",
    title: "Карточка игрока",
    keywords: "профиль карточка баны часы ники связи серверы",
    body: (
      <>
        <ul className="ml-4 list-disc space-y-1">
          <li><b>Обзор</b> — дата создания аккаунта, часы в Rust (если игрок их не скрыл), история ников, последние сессии, заметка для себя.</li>
          <li><b>Активность</b> — когда игрок обычно играет (подробно ниже).</li>
          <li><b>Серверы</b> — где источник видел игрока. Это не полный список: только наблюдаемые серверы.</li>
          <li><b>Баны</b> — Steam (VAC, Game, Community, Trade) отдельно от банов проектов.</li>
          <li><b>Бой</b> — K/D и точность. Ни один доступный источник этого не даёт, поэтому раздел честно пуст.</li>
          <li><b>Связи</b> — с кем игрок регулярно бывает на одном сервере в одно время. Это подсказка, а не доказательство дружбы.</li>
        </ul>
        <p className="mt-2">
          Возле каждого блока есть метка источника: <b>подтверждено</b>, <b>устарело</b> (старше суток), <b>частично</b>, <b>не настроено</b>, <b>демо</b>. Кнопка «Обновить» заново запрашивает Steam.
        </p>
      </>
    ),
  },
  {
    id: "activity",
    title: "Как читать активность и тепловую карту",
    keywords: "активность тепловая карта heatmap покрытие неизвестно офлайн часы сон",
    body: (
      <>
        <Steps
          items={[
            <>Тепловая карта — 7 дней недели × 24 часа. Чем ярче бирюзовый, тем чаще игрок был онлайн в этот час.</>,
            <>
              <b>Штриховка = нет данных.</b> Программа в это время не наблюдала игрока (ПК был выключен, трекер на паузе). Это <u>не</u> «офлайн».
            </>,
            <>Точки = мало данных (меньше 30 минут наблюдений за период) — выводы делать рано.</>,
            <>«Покрытие» сверху — какую долю периода программа реально наблюдала. При покрытии меньше 30% картина ненадёжна.</>,
            <>Жёлтая рамка — <b>типичные игровые офлайн-часы</b>: часы, когда игрок стабильно не в игре. Показываются, только если в каждой ячейке есть минимум 3 разных дня наблюдений.</>,
            <>Наведите на ячейку — внизу появится расшифровка: сколько минут онлайн из скольких наблюдаемых и за сколько дней.</>,
          ]}
        />
        <Notice tone="warning" className="mt-3">
          Время входа/выхода известно с точностью до интервала опроса (например, «≈ 21:00–21:02»). Отсутствие на наблюдаемых серверах не значит, что человек спит или не играет в Rust на другом сервере.
        </Notice>
      </>
    ),
  },
  {
    id: "tracker",
    title: "Live-трекер: следить, кто сейчас в игре",
    keywords: "трекер следить онлайн уведомления опрос пауза трей",
    body: (
      <>
        <Steps
          items={[
            <>Откройте карточку игрока → включите <b>Live-трекер</b>.</>,
            <>Каждые 2 минуты (меняется в Настройках) программа спрашивает Steam, запущен ли у игрока Rust.</>,
            <>Изменения попадают в журнал на странице <To to="/tracker">Live-трекер</To>: «Запустил Rust», «Вышел из Rust», «Сменил ник», «Изменение банов».</>,
            <>Зелёная точка — онлайн, серая — не в игре, штриховка — неизвестно (источник не ответил или опроса ещё не было).</>,
            <>Колокольчик у игрока включает/выключает уведомления именно по нему; «Пауза всего» останавливает все опросы.</>,
          ]}
        />
        <Notice tone="info" className="mt-3">
          Трекер работает, только пока программа запущена. Чтобы она не закрывалась вместе с окном — Настройки → «Закрытие окна сворачивает в трей», и при желании «Запускать вместе с Windows».
          Выключенный или спящий ПК ничего не наблюдает.
        </Notice>
        <p className="mt-2 text-muted">
          Ограничение Steam: он сообщает только «в Rust / не в Rust», без сервера. Если у игрока закрыт профиль — состояние «неизвестно». Если игрок в режиме «Невидимка» в Steam — он будет выглядеть как «не в
          игре», отличить это невозможно.
        </p>
      </>
    ),
  },
  {
    id: "notify",
    title: "Уведомления Windows и Telegram",
    keywords: "уведомления telegram бот chat id тихие часы windows",
    body: (
      <>
        <h3 className="font-semibold">Windows</h3>
        <p>
          <To to="/settings">Настройки</To> → «Уведомления» → включить «Уведомления Windows» → «Тест». Если тест не пришёл — проверьте, не отключены ли уведомления в Windows (Параметры → Система → Уведомления).
        </p>
        <h3 className="mt-3 font-semibold">Telegram (на телефон)</h3>
        <Steps
          items={[
            <>В Telegram откройте <b>@BotFather</b> → <code>/newbot</code> → придумайте имя → получите токен вида <code>123456:ABC…</code>.</>,
            <>Найдите своего нового бота и отправьте ему любое сообщение (иначе бот не сможет вам писать).</>,
            <>Узнайте свой chat ID: напишите боту <b>@userinfobot</b> — он ответит числом.</>,
            <>
              <To to="/integrations">Интеграции</To> → «Telegram» → вставить токен → «Сохранить»; ниже вставить chat ID → «Сохранить» → «Тест».
            </>,
            <>Настройки → «Уведомления» → включить «Telegram».</>,
          ]}
        />
        <p className="mt-2">Там же — <b>тихие часы</b> (например, 23:00–08:00) и выбор, о каких событиях сообщать.</p>
      </>
    ),
  },
  {
    id: "squads",
    title: "Отряды (кланы, тиммейты)",
    keywords: "отряд клан группа тиммейты команда все офлайн",
    body: (
      <>
        <Steps
          items={[
            <>
              <To to="/squads">Отряды</To> → «Новый отряд» → название и цвет.
            </>,
            <>Добавьте участников (до 20) из списка уже найденных игроков — или из карточки игрока «+ в отряд».</>,
            <>Совместная шкала показывает, кто из отряда когда был онлайн; полоса «Отряд» — итог.</>,
            <>«Все наблюдались офлайн» засчитывается, только если есть данные по каждому участнику. Нет данных хотя бы по одному — «неизвестно».</>,
            <>«Возможные тиммейты» — игроки, которые ≥ 3 раз были на том же сервере в то же время. Требует истории серверов (BattleMetrics.com или демо).</>,
          ]}
        />
      </>
    ),
  },
  {
    id: "bans",
    title: "Баны и индикаторы риска",
    keywords: "бан vac game ban риск читер",
    body: (
      <>
        <ul className="ml-4 list-disc space-y-1">
          <li><b>VAC</b> — античит Valve (в других играх Valve). <b>Game Ban</b> — бан от разработчика какой-то игры; Steam не говорит, какой именно, это не обязательно Rust.</li>
          <li><b>Community / Trade</b> — ограничения сообщества и обмена Steam.</li>
          <li><b>Баны проектов</b> (конкретных серверов) — пока не подключены: у серверов Rust нет открытых документированных API.</li>
          <li>
            <b>Индикаторы риска</b> — простая сумма публичных признаков (баны, возраст аккаунта, мало часов, закрытый профиль) с весами. Это не обвинение в читах. Отключается в Настройках.
          </li>
        </ul>
      </>
    ),
  },
  {
    id: "servers",
    title: "Серверы и вайпы",
    keywords: "сервер вайп онлайн каталог",
    body: (
      <>
        <p>
          Нужен ключ battlemetrics.ru. <To to="/servers">Серверы</To> → введите часть названия → «Обновить каталог». Звёздочка добавляет сервер в отслеживаемые (они видны на Обзоре).
        </p>
        <p className="mt-2">
          Справа — график онлайна сервера за 24 ч / 7 / 30 дней и даты вайпов. Метка «подозрительный» значит, что источник не подтвердил реальный онлайн, поэтому число игроков скрыто.
        </p>
        <p className="mt-2 text-muted">График онлайна сервера — общий для сервера, он не показывает, какие именно игроки там были.</p>
      </>
    ),
  },
  {
    id: "data",
    title: "Данные, резервные копии, удаление",
    keywords: "экспорт импорт бэкап резервная копия удалить данные csv",
    body: (
      <>
        <p>
          <To to="/settings">Настройки</To> → «Данные»:
        </p>
        <ul className="ml-4 list-disc space-y-0.5">
          <li><b>Экспорт JSON</b> — всё в один файл (без ключей); <b>CSV</b> — сессии или события для Excel.</li>
          <li><b>Резервная копия</b> — копия базы целиком; <b>Восстановить</b> — вернуть её (текущая база сохраняется рядом).</li>
          <li><b>Диагностика</b> — проверка целостности базы и состояние источников.</li>
          <li><b>Удалить все данные</b> — полная очистка текущего режима (нужно ввести УДАЛИТЬ). Ключи при этом остаются.</li>
        </ul>
        <p className="mt-2">Старые сырые наблюдения удаляются автоматически по сроку хранения (по умолчанию 90 дней).</p>
      </>
    ),
  },
  {
    id: "keys-hotkeys",
    title: "Горячие клавиши",
    keywords: "клавиши hotkey ctrl alt",
    body: (
      <ul className="space-y-1">
        <li>
          <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> — быстрый поиск игрока или раздела
        </li>
        <li>
          <Kbd>Alt</Kbd>+<Kbd>1</Kbd>…<Kbd>9</Kbd> — переход по разделам меню
        </li>
        <li>
          <Kbd>↑</Kbd> <Kbd>↓</Kbd> <Kbd>Enter</Kbd> <Kbd>Esc</Kbd> — навигация в быстром поиске
        </li>
        <li>Переключатель «UTC» вверху справа — показывать время в UTC вместо вашего часового пояса.</li>
      </ul>
    ),
  },
  {
    id: "faq",
    title: "Частые вопросы",
    keywords: "вопросы faq eac античит безопасно бан проблема",
    body: (
      <div className="space-y-3">
        <div>
          <b>Не забанят ли меня в Rust (EAC)?</b>
          <p>Нет. Программа не трогает игру и её файлы, ничего не внедряет и не читает память — только запрашивает открытые сайты Steam/BattleMetrics через интернет.</p>
        </div>
        <div>
          <b>Почему у игрока «неизвестно», хотя он играет?</b>
          <p>Закрытый профиль Steam, «Невидимка», трекер на паузе или программа была закрыта. Проверьте в карточке видимость профиля и на странице трекера время последнего успешного опроса.</p>
        </div>
        <div>
          <b>Почему не видно, на каком сервере игрок?</b>
          <p>Steam этого не сообщает. Сервер знает только BattleMetrics.com (платная подписка).</p>
        </div>
        <div>
          <b>Пишет «не настроено».</b>
          <p>Не введён ключ нужного источника — см. раздел «Ключи».</p>
        </div>
        <div>
          <b>Windows ругается при запуске (SmartScreen).</b>
          <p>У программы нет платной цифровой подписи. Нажмите «Подробнее» → «Выполнить в любом случае». Если не запускается вообще — установите Microsoft Edge WebView2 Runtime с сайта Microsoft.</p>
        </div>
        <div>
          <b>Где лежат мои данные и как всё удалить?</b>
          <p>
            В папке <code>%APPDATA%\dev.rid.desktop</code>. Удалите её, чтобы стереть всё; ключи удаляются кнопкой «Удалить» в Интеграциях.
          </p>
        </div>
      </div>
    ),
  },
];

export function HelpPage() {
  const [q, setQ] = useState("");
  const list = useMemo(() => {
    const t = q.trim().toLowerCase();
    if (!t) return SECTIONS;
    return SECTIONS.filter((s) => (s.title + " " + s.keywords).toLowerCase().includes(t));
  }, [q]);
  const go = (id: string) => document.getElementById(`help-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  return (
    <div className="space-y-4">
      <PageHeader title="Инструкция" subtitle="Всё, что нужно, чтобы начать: ключи, поиск, трекер, уведомления, отряды" />
      <div className="grid gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
        <Card className="h-fit lg:sticky lg:top-0">
          <div className="border-b border-line p-2">
            <div className="relative">
              <Search className="pointer-events-none absolute top-2 left-2.5 size-4 text-faint" />
              <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Поиск по инструкции" className="pl-8" aria-label="Поиск по инструкции" />
            </div>
          </div>
          <nav className="p-1.5" aria-label="Разделы инструкции">
            {SECTIONS.map((s) => (
              <button
                key={s.id}
                type="button"
                onClick={() => go(s.id)}
                className={cn("block w-full truncate rounded-md px-2.5 py-1.5 text-left text-[13px] text-muted hover:bg-raised hover:text-fg", !list.includes(s) && "opacity-40")}
              >
                {s.title}
              </button>
            ))}
          </nav>
        </Card>
        <div className="min-w-0 space-y-4">
          {list.length === 0 && <Notice>Ничего не найдено по запросу «{q}».</Notice>}
          {list.map((s) => (
            <Card key={s.id} id={`help-${s.id}`} className="scroll-mt-4">
              <h2 className="flex items-center gap-2 border-b border-line px-4 py-2.5 font-display text-sm font-semibold">
                <BookOpen className="size-4 text-teal" />
                {s.title}
              </h2>
              <div className="space-y-2 px-4 py-3 text-[13px] leading-relaxed">{s.body}</div>
            </Card>
          ))}
        </div>
      </div>
    </div>
  );
}
