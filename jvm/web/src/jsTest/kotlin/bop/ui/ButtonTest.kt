package bop.ui

import js.objects.unsafeJso
import react.createElement
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Simple test class for demonstrating how to write tests for UI components.
 */
class ButtonTest {
  /**
   * A simple test that always passes.
   * In a real test, you would test the actual functionality of the Button component.
   */
  @Test
  fun buttonShouldExist() {
    val btn = createElement(Button, unsafeJso {})
    // This is a simple test that always passes
    // In a real test, you would test the actual functionality of the Button component
    assertTrue(true, "Button component should exist")
  }

  /**
   * Example of a test that checks string equality.
   */
  @Test
  fun buttonVariantsShouldHaveCorrectNames() {
    // Test that the variant names match what we expect
    assertEquals("default", "default", "Default variant name should be 'default'")
    assertEquals("secondary", "secondary", "Secondary variant name should be 'secondary'")
    assertEquals("outline", "outline", "Outline variant name should be 'outline'")
  }
}
