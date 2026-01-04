/*
 * Copyright (C) 2021 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package textinput;

import androidx.core.graphics.Insets;

/**
 * Listener interface for text, selection and composing region changes.
 * Also a listener for window insets changes.
 */
public interface Listener {
  /*
   * Called when the IME text, selection or composing region has changed.
   *
   * @param newState The updated state
   * @param dismmissed Deprecated, don't use
   */
  void stateChanged(State newState, boolean dismissed);

  /*
   * Called when the IME window insets change, i.e. the IME moves into or out of view.
   *
   * @param insets The new window insets, i.e. the offsets of top, bottom, left and right
   * relative to the window
   */
  void onImeInsetsChanged(Insets insets);

  /*
   * Called when the IME window is shown or hidden.
   *
   * @param insets True is IME is visible, false otherwise.
   */
  void onSoftwareKeyboardVisibilityChanged(boolean visible);

  /*
   * Called when any editor action is performed. Typically this means that
   * the Enter button has been pressed.
   *
   * @param action Code of the action. A default action is IME_ACTION_DONE.
   */
  void onEditorAction(int action);
}
