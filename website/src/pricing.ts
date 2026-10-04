// Plans and prices shown on the site. This is the only place prices live:
// change a number here and the pricing section, the yearly discount and the
// account page all follow.
//
// There's no checkout yet. Choosing a plan opens the sample account page with
// ?plan=<id>&billing=<monthly|yearly>, which a real checkout will replace.

export type Billing = "monthly" | "yearly";

export interface Plan {
  id: string;
  name: string;
  /** One line on who the plan is for. */
  audience: string;
  /** Price per month when billed monthly, in whole US dollars. */
  monthly: number;
  /** Price for a full year when billed yearly, in whole US dollars. */
  yearly: number;
  features: string[];
  /** A small line under the price, for add-ons. */
  note?: string;
  /** The plan most churches should pick; drawn with the primary button. */
  featured?: boolean;
}

/**
 * While true, the section says the prices are samples. Prices were approved
 * on 2026-10-04.
 */
export const PRICES_ARE_PLACEHOLDERS = false;

/**
 * Launch offer: the first `spots` churches keep `percentOff` for as long as
 * they stay subscribed. Set to null to take it off the site.
 */
export const FOUNDING_OFFER: { percentOff: number; spots: number } | null = { percentOff: 25, spots: 50 };

/** A price after the founding-church discount, to the cent. */
export const foundingPrice = (amount: number) =>
  FOUNDING_OFFER ? Math.round(amount * (100 - FOUNDING_OFFER.percentOff)) / 100 : amount;

export const CURRENCY = "USD";

export const TRIAL_DAYS = 30;

/** Shown under the plans. */
export const PRICING_FOOTNOTE =
  "Receiving Dante on a Mac may need Audinate's Dante Virtual Soundcard, sold separately by Audinate.";

export const PLANS: Plan[] = [
  {
    id: "essentials",
    name: "Essentials",
    audience: "One room with a smaller band, up to 32 input channels.",
    monthly: 49,
    yearly: 490,
    features: [
      "1 console, up to 32 input channels",
      "AI auto-mix for levels and balance",
      "6 room presets",
      "Local scenes and mix log",
      "Instant manual takeover",
      "Up to 3 team logins",
      "Email support",
    ],
  },
  {
    id: "pro",
    name: "Pro",
    audience: "One room using the whole console.",
    monthly: 99,
    yearly: 990,
    featured: true,
    features: [
      "Everything in Essentials",
      "All console inputs (dLive up to 128)",
      "Unlimited volunteer logins with Admin, Engineer and Volunteer roles",
      "Cloud backup and sync across Macs",
      "Mix report after every service",
      "AI EQ assist (coming soon)",
      "Priority support",
    ],
  },
  {
    id: "campus",
    name: "Campus",
    audience: "Up to 3 rooms or consoles under one church.",
    monthly: 249,
    yearly: 2490,
    note: "Extra rooms $59 a month ($590 a year) each.",
    features: [
      "Everything in Pro",
      "Up to 3 rooms or consoles",
      "Shared scene library across rooms",
      "Onboarding call",
      "Fastest support",
    ],
  },
];

export const findPlan = (id: string | null) => PLANS.find((p) => p.id === id);

/** Whole-number percent saved by paying yearly instead of twelve months. */
export const yearlySavingsPercent = (plan: Plan) => Math.round((1 - plan.yearly / (plan.monthly * 12)) * 100);

/** The best yearly saving across plans, for the toggle label. */
export const maxYearlySavingsPercent = () => Math.max(...PLANS.map(yearlySavingsPercent));

/** Whole months saved by paying yearly, when every plan saves the same (e.g. 2). */
export function monthsFreeYearly() {
  const months = PLANS.map((p) => 12 - p.yearly / p.monthly);
  return months.every((m) => m === months[0] && Number.isInteger(m)) ? months[0] : 0;
}

const money = new Intl.NumberFormat("en-US", { style: "currency", currency: CURRENCY, maximumFractionDigits: 0 });
const moneyCents = new Intl.NumberFormat("en-US", { style: "currency", currency: CURRENCY, minimumFractionDigits: 2 });

/** "$29", or "$24.17" when a yearly price doesn't divide evenly by month. */
export function formatPrice(amount: number) {
  return Number.isInteger(amount) ? money.format(amount) : moneyCents.format(amount);
}

/** The per-month figure shown large on a plan card. */
export const pricePerMonth = (plan: Plan, billing: Billing) =>
  billing === "monthly" ? plan.monthly : Math.round((plan.yearly / 12) * 100) / 100;

/** Link from a plan card into the sample account flow. */
export const planHref = (root: string, plan: Plan, billing: Billing) =>
  `${root}account/?plan=${encodeURIComponent(plan.id)}&billing=${billing}`;
