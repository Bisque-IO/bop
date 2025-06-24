@file:JsModule("react-day-picker") @file:JsNonModule

package lib.dayPicker

import js.date.Date
import react.*

external interface Interval {
   var start: Date
   var end: Date
}

external interface DayPickerProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var mode: String? // "single" | "multiple" | "range"
   var selected: dynamic
   var defaultSelected: dynamic
   var navLayout: String?
   var onSelect: ((dynamic) -> Unit)?
   var hideNavigation: Boolean?
   var required: Boolean?
   var classNames: dynamic
   var styles: dynamic
   var components: dynamic
   var locale: dynamic
   var weekStartsOn: Int?
   var numberOfMonths: Int?
   var defaultMonth: Date?
   var month: Date?
   var startMonth: Date?
   var endMonth: Date?
   var onMonthChange: ((Date) -> Unit)?
   var onNextClick: ((Date) -> Unit)?
   var onPrevClick: ((Date) -> Unit)?
   var onDayClick: ((Date, dynamic, dynamic) -> Unit)?
   var onDayFocus: ((Date, dynamic, dynamic) -> Unit)?
   var onDayBlur: ((Date, dynamic, dynamic) -> Unit)?
   var onDayKeyDown: ((Date, dynamic, dynamic) -> Unit)?
   var onDayMouseEnter: ((Date, dynamic, dynamic) -> Unit)?
   var onDayMouseLeave: ((Date, dynamic, dynamic) -> Unit)?
   var disabled: dynamic
   var hidden: dynamic
   var modifiers: dynamic
   var modifiersClassNames: dynamic
   var modifiersStyles: dynamic
   var footer: ReactNode?
   var captionLayout: String? // "dropdown" | "buttons"
   var showOutsideDays: Boolean?
   var fixedWeeks: Boolean?
   var showWeekNumber: Boolean?
   var ISOWeek: Boolean?
   var firstWeekContainsDate: Int?
   var labels: dynamic
   var formatters: dynamic
   var dir: String? // "ltr" | "rtl"
   var lang: String?
   var ariaLabel: String?
   var id: String?
   var nonce: String?
   var autoFocus: Boolean?
   var animate: Boolean?
   var broadcastCalendar: Boolean?
   var dateLib: dynamic
   var numerals: dynamic
}

@JsName("DayPicker")
external val DayPicker: ComponentType<DayPickerProps>

external interface Locale {
   var code: String
   var formatDistance: (token: String, count: Number, options: dynamic) -> String
}

external interface FormatDateOptions {
   var firstWeekContainsDate: Int? // 1 | 4
   var `in`: dynamic
   var locale: dynamic // Locale | "options" | "localize" | "formatLong"
   var useAdditionalDayOfYearTokens: Boolean?
   var useAdditionalWeekYearTokens: Boolean?
   var weekStartsOn: Int
}

external interface DateLib {
   val options: dynamic

   fun formatNumber(value: Number): String
   fun formatNumber(value: String): String

   /**
    * Creates a new `Date` object representing today's date.
    */
   val today: () -> Date

   /**
    * Creates a new `Date` object with the specified year, month, and day.
    *
    * @since 9.5.0
    * @param year The year.
    * @param monthIndex The month (0-11).
    * @param date The day of the month.
    * @returns A new `Date` object.
    */
   val newDate: (year: Number, monthIndex: Number, date: Number) -> Date

   /**
    * Adds the specified number of days to the given date.
    *
    * @param date The date to add days to.
    * @param amount The number of days to add.
    * @returns The new date with the days added.
    */
   val addDays: (date: Date, amount: Number) -> Date

   /**
    * Adds the specified number of months to the given date.
    *
    * @param date The date to add months to.
    * @param amount The number of months to add.
    * @returns The new date with the months added.
    */
   val addMonths: (date: Date, amount: Number) -> Date

   /**
    * Adds the specified number of weeks to the given date.
    *
    * @param date The date to add weeks to.
    * @param amount The number of weeks to add.
    * @returns The new date with the weeks added.
    */
   val addWeeks: (date: Date, amount: Number) -> Date

   /**
    * Adds the specified number of years to the given date.
    *
    * @param date The date to add years to.
    * @param amount The number of years to add.
    * @returns The new date with the years added.
    */
   val addYears: (date: Date, amount: Number) -> Date

   /**
    * Returns the number of calendar days between the given dates.
    *
    * @param dateLeft The later date.
    * @param dateRight The earlier date.
    * @returns The number of calendar days between the dates.
    */
   val differenceInCalendarDays: (dateLeft: Date, dateRight: Date) -> Int

   /**
    * Returns the number of calendar months between the given dates.
    *
    * @param dateLeft The later date.
    * @param dateRight The earlier date.
    * @returns The number of calendar months between the dates.
    */
   val differenceInCalendarMonths: (dateLeft: Date, dateRight: Date) -> Int

   /**
    * Returns the months between the given dates.
    *
    * @param interval The interval to get the months for.
    */
   val eachMonthOfInterval: (interval: Interval) -> Array<Date>

   /**
    * Returns the end of the broadcast week for the given date.
    *
    * @param date The original date.
    * @returns The end of the broadcast week.
    */
   val endOfBroadcastWeek: (date: Date) -> Date

   /**
    * Returns the end of the ISO week for the given date.
    *
    * @param date The original date.
    * @returns The end of the ISO week.
    */
   val endOfISOWeek: (date: Date) -> Date

   /**
    * Returns the end of the month for the given date.
    *
    * @param date The original date.
    * @returns The end of the month.
    */
   val endOfMonth: (date: Date) -> Date

   /**
    * Returns the end of the week for the given date.
    *
    * @param date The original date.
    * @returns The end of the week.
    */
   val endOfWeek: (date: Date, options: dynamic? /*EndOfWeekOptions<Date>*/) -> Date

   /**
    * Returns the end of the year for the given date.
    *
    * @param date The original date.
    * @returns The end of the year.
    */
   val endOfYear: (date: Date) -> Date

   /**
    * Formats the given date using the specified format string.
    *
    * @param date The date to format.
    * @param formatStr The format string.
    * @returns The formatted date string.
    */
   val format: (date: Date, formatStr: String, options: FormatDateOptions?) -> String

   /**
    * Returns the ISO week number for the given date.
    *
    * @param date The date to get the ISO week number for.
    * @returns The ISO week number.
    */
   val getISOWeek: (date: Date) -> Int

   /**
    * Returns the month of the given date.
    *
    * @param date The date to get the month for.
    * @returns The month.
    */
   val getMonth: (date: Date, options: dynamic?) -> Int

   /**
    * Returns the year of the given date.
    *
    * @param date The date to get the year for.
    * @returns The year.
    */
   val getYear: (date: Date, options: dynamic?) -> Int

   /**
    * Returns the local week number for the given date.
    *
    * @param date The date to get the week number for.
    * @returns The week number.
    */
   val getWeek: (date: Date, options: dynamic?) -> Int

   /**
    * Checks if the first date is after the second date.
    *
    * @param date The date to compare.
    * @param dateToCompare The date to compare with.
    * @returns True if the first date is after the second date.
    */
   val isAfter: (date: Date, dateToCompare: Date) -> Boolean

   /**
    * Checks if the first date is before the second date.
    *
    * @param date The date to compare.
    * @param dateToCompare The date to compare with.
    * @returns True if the first date is before the second date.
    */
   val isBefore: (date: Date, dateToCompare: Date) -> Boolean

   /**
    * Checks if the given value is a Date object.
    *
    * @param value The value to check.
    * @returns True if the value is a Date object.
    */
   val isDate: (value: dynamic) -> Boolean

   /**
    * Checks if the given dates are on the same day.
    *
    * @param dateLeft The first date to compare.
    * @param dateRight The second date to compare.
    * @returns True if the dates are on the same day.
    */
   val isSameDay: (dateLeft: Date, dateRight: Date) -> Boolean

   /**
    * Checks if the given dates are in the same month.
    *
    * @param dateLeft The first date to compare.
    * @param dateRight The second date to compare.
    * @returns True if the dates are in the same month.
    */
   val isSameMonth: (dateLeft: Date, dateRight: Date) -> Boolean

   /**
    * Checks if the given dates are in the same year.
    *
    * @param dateLeft The first date to compare.
    * @param dateRight The second date to compare.
    * @returns True if the dates are in the same year.
    */
   val isSameYear: (dateLeft: Date, dateRight: Date) -> Boolean

   /**
    * Returns the latest date in the given array of dates.
    *
    * @param dates The array of dates to compare.
    * @returns The latest date.
    */
   val max: (dates: Array<Date>) -> Date

   /**
    * Returns the earliest date in the given array of dates.
    *
    * @param dates The array of dates to compare.
    * @returns The earliest date.
    */
   val min: (dates: Array<Date>) -> Date

   /**
    * Sets the month of the given date.
    *
    * @param date The date to set the month on.
    * @param month The month to set (0-11).
    * @returns The new date with the month set.
    */
   val setMonth: (date: Date, month: Int) -> Date

   /**
    * Sets the year of the given date.
    *
    * @param date The date to set the year on.
    * @param year The year to set.
    * @returns The new date with the year set.
    */
   val setYear: (date: Date, year: Int) -> Date

   /**
    * Returns the start of the broadcast week for the given date.
    *
    * @param date The original date.
    * @returns The start of the broadcast week.
    */
   val startOfBroadcastWeek: (date: Date, dateLib: DateLib) -> Date

   /**
    * Returns the start of the day for the given date.
    *
    * @param date The original date.
    * @returns The start of the day.
    */
   val startOfDay: (date: Date) -> Date

   /**
    * Returns the start of the ISO week for the given date.
    *
    * @param date The original date.
    * @returns The start of the ISO week.
    */
   val startOfISOWeek: (date: Date) -> Date

   /**
    * Returns the start of the month for the given date.
    *
    * @param date The original date.
    * @returns The start of the month.
    */
   val startOfMonth: (date: Date) -> Date

   /**
    * Returns the start of the week for the given date.
    *
    * @param date The original date.
    * @returns The start of the week.
    */
   val startOfWeek: (date: Date, options: dynamic? /*StartOfWeekOptions*/) -> Date

   /**
    * Returns the start of the year for the given date.
    *
    * @param date The original date.
    * @returns The start of the year.
    */
   val startOfYear: (date: Date) -> Date
}

@JsName("defaultDateLib")
external val defaultDateLib: DateLib

external interface Modifiers {
   var today: Boolean?
   var selected: Boolean?
   var weekend: Boolean?
}

external interface CalendarDay {
   val dateLib: DateLib
   val outside: Boolean
   val displayMonth: Date
   val date: Date

   fun isEqualTo(day: CalendarDay): Boolean
}

external interface CalendarWeek {
   val days: Array<CalendarDay>
   val weekNumber: Int
}

external interface CalendarMonth {
   val date: Date
   val weeks: Array<CalendarWeek>
}

external interface SelectedValue

external interface DayPickerContext {
   val classNames: dynamic?
   val components: dynamic?
   val dayPickerProps: DayPickerProps
   val formatters: dynamic?

   val getModifiers: (day: CalendarDay) -> Modifiers

   val goToMonth: (month: Date) -> Unit

   val selected: dynamic?

   val select: dynamic?

   val isSelected: ((date: Date) -> Boolean)?
   val styles: dynamic?
   val labels: dynamic?

   val nextMonth: Date?
   val previousMonth: Date?
   val months: Array<dynamic>
}

@JsName("useDayPicker")
external fun useDayPicker(): DayPickerContext
