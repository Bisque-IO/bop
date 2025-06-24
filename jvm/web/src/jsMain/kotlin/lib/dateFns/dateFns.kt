@file:JsModule("date-fns") @file:JsNonModule

package lib.dateFns

import js.date.Date


@JsName("addDays")
external fun addDays(date: Date, days: Number): Date