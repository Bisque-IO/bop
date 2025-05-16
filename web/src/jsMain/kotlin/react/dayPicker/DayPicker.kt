@file:JsModule("react-day-picker")
@file:JsNonModule

package react.dayPicker

import react.ComponentType
import react.PropsWithChildren
import react.ReactNode
import web.cssom.ClassName
import kotlin.js.Date

external interface DayPickerProps : PropsWithChildren {
    var mode: String? // "single" | "multiple" | "range"
    var selected: dynamic
    var defaultSelected: dynamic
    var onSelect: ((dynamic) -> Unit)?
    var required: Boolean?
    var className: ClassName?
    var classNames: dynamic?
    var styles: dynamic
    var components: dynamic
    var locale: dynamic
    var weekStartsOn: Int?
    var numberOfMonths: Int?
    var defaultMonth: Date?
    var month: Date?
    var onMonthChange: ((Date) -> Unit)?
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
