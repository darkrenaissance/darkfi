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

import android.text.Editable;
import android.text.Spanned;
import android.view.inputmethod.EditorInfo;

/*
 * Singleton GameTextInput class with helper methods.
 */
public final class GameTextInput {
  public final static void copyEditorInfo(EditorInfo from, EditorInfo to) {
    if (from == null || to == null)
      return;
    if (from.hintText != null) {
      to.hintText = from.hintText;
    }

    to.inputType = from.inputType;
    to.imeOptions = from.imeOptions;
    to.label = from.label;
    to.initialCapsMode = from.initialCapsMode;
    to.privateImeOptions = from.privateImeOptions;
    if (from.packageName != null) {
      to.packageName = from.packageName;
    }

    to.fieldId = from.fieldId;
    if (from.fieldName != null) {
      to.fieldName = from.fieldName;
    }

    to.initialSelStart = from.initialSelStart;
    to.initialSelEnd = from.initialSelEnd;
  }

  public static final class Pair {
    int first, second;

    Pair(int f, int s) {
      first = f;
      second = s;
    }
  }

  private GameTextInput() {}
}
