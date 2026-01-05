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

import static android.view.inputmethod.EditorInfo.IME_ACTION_UNSPECIFIED;

import android.app.Activity;
import android.content.Context;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputFilter;
import android.text.Selection;
import android.text.SpannableString;
import android.text.SpannableStringBuilder;
import android.text.Spanned;
import android.text.TextUtils;
import android.util.Log;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.CompletionInfo;
import android.view.inputmethod.CorrectionInfo;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.InputMethodManager;
import android.graphics.Insets;
import android.view.WindowInsets;

import textinput.GameTextInput.Pair;

public class InputConnection extends BaseInputConnection implements View.OnKeyListener {
  private final InputMethodManager imm;
  private final View targetView;
  private final Settings settings;
  private final Editable mEditable;
  private Listener listener;
  private boolean mSoftKeyboardActive;

  private void log(String text) {
    //Log.d("darkfi", text);
  }

  /*
   * This class filters EOL characters from the input. For details of how InputFilter.filter
   * function works, refer to its documentation. If the suggested change is accepted without
   * modifications, filter() should return null.
   */
  private class SingeLineFilter implements InputFilter {
    public CharSequence filter(
        CharSequence source, int start, int end, Spanned dest, int dstart, int dend) {
      boolean keepOriginal = true;
      StringBuilder builder = new StringBuilder(end - start);

      for (int i = start; i < end; i++) {
        char c = source.charAt(i);

        if (c == '\n') {
          keepOriginal = false;
        } else {
          builder.append(c);
        }
      }

      if (keepOriginal) {
        return null;
      }

      if (source instanceof Spanned) {
        SpannableString s = new SpannableString(builder);
        TextUtils.copySpansFrom((Spanned) source, start, builder.length(), null, s, 0);
        return s;
      } else {
        return builder;
      }
    }
  }

  private static final int MAX_LENGTH_FOR_SINGLE_LINE_EDIT_TEXT = 5000;

  /**
   * Constructor
   *
   * @param ctx        The app's context
   * @param targetView The view created this input connection
   * @param settings   EditorInfo and other settings needed by this class
   *                   InputConnection.
   */
  public InputConnection(Context ctx, View targetView, Settings settings) {
    super(targetView, settings.mEditorInfo.inputType != 0);
    log("InputConnection created");

    this.targetView = targetView;
    this.settings = settings;
    Object imm = ctx.getSystemService(Context.INPUT_METHOD_SERVICE);
    if (imm == null) {
      throw new java.lang.RuntimeException("Can't get IMM");
    } else {
      this.imm = (InputMethodManager) imm;
      this.mEditable = (Editable) (new SpannableStringBuilder());
    }
    // Listen for insets changes
    targetView.setOnKeyListener(this);
    // Apply EditorInfo settings
    this.setEditorInfo(settings.mEditorInfo);
  }

  /**
   * Restart the input method manager. This is useful to apply changes to the keyboard
   * after calling setEditorInfo.
   */
  public void restartInput() {
    imm.restartInput(targetView);
  }

  /**
   * Get whether the soft keyboard is visible.
   *
   * @return true if the soft keyboard is visible, false otherwise
   */
  public final boolean getSoftKeyboardActive() {
    return this.mSoftKeyboardActive;
  }

  /**
   * Request the soft keyboard to become visible or invisible.
   *
   * @param active True if the soft keyboard should be made visible, otherwise false.
   * @param flags  See
   *     https://developer.android.com/reference/android/view/inputmethod/InputMethodManager#showSoftInput(android.view.View,%20int)
   */
  public final void setSoftKeyboardActive(boolean active, int flags) {
    log("setSoftKeyboardActive, active: " + active);

    this.mSoftKeyboardActive = active;
    if (active) {
      this.targetView.setFocusableInTouchMode(true);
      this.targetView.requestFocus();
      this.imm.showSoftInput(this.targetView, flags);
    } else {
      this.imm.hideSoftInputFromWindow(this.targetView.getWindowToken(), flags);
    }
    restartInput();
  }

  /**
   * Get the current EditorInfo used to configure the InputConnection's behaviour.
   *
   * @return The current EditorInfo.
   */
  public final EditorInfo getEditorInfo() {
    return this.settings.mEditorInfo;
  }

  /**
   * Set the current EditorInfo used to configure the InputConnection's behaviour.
   *
   * @param editorInfo The EditorInfo to use
   */
  public final void setEditorInfo(EditorInfo editorInfo) {
    log("setEditorInfo");
    settings.mEditorInfo = editorInfo;

    // Depending on the multiline state, we might need a different set of filters.
    // Filters are being used to filter specific characters for hardware keyboards
    // (software input methods already support TYPE_TEXT_FLAG_MULTI_LINE).
    if ((settings.mEditorInfo.inputType & EditorInfo.TYPE_TEXT_FLAG_MULTI_LINE) == 0) {
      mEditable.setFilters(
          new InputFilter[] {new InputFilter.LengthFilter(MAX_LENGTH_FOR_SINGLE_LINE_EDIT_TEXT),
              new SingeLineFilter()});
    } else {
      mEditable.setFilters(new InputFilter[] {});
    }
  }

  /**
   * Set the text, selection and composing region state.
   *
   * @param state The state to be used by the IME.
   *              This replaces any text, selections and composing regions currently active.
   */
  public final void setState(State state) {
    if (state == null)
      return;
    log(
        "setState: '" + state.text + "', selection=(" + state.selectionStart + ","
            + state.selectionEnd + "), composing region=(" + state.composingRegionStart + ","
            + state.composingRegionEnd + ")");
    mEditable.clear();
    mEditable.clearSpans();
    mEditable.insert(0, (CharSequence) state.text);
    setSelection(state.selectionStart, state.selectionEnd);
    if (state.composingRegionStart != state.composingRegionEnd) {
      setComposingRegion(state.composingRegionStart, state.composingRegionEnd);
    }
    restartInput();
  }

  /**
   * Get the current listener for state changes.
   *
   * @return The current Listener
   */
  public final Listener getListener() {
    return listener;
  }

  /**
   * Set a listener for state changes.
   *
   * @param listener
   * @return This InputConnection, for setter chaining.
   */
  public final InputConnection setListener(Listener listener) {
    this.listener = listener;
    return this;
  }

  // From View.OnKeyListener
  @Override
  public boolean onKey(View view, int i, KeyEvent keyEvent) {
    log("onKey: " + keyEvent);
    if (!getSoftKeyboardActive()) {
      return false;
    }
    // Don't call sendKeyEvent as it might produce an infinite loop.
    if (processKeyEvent(keyEvent)) {
      // IMM seems to cache the content of Editable, so we update it with restartInput
      // Also it caches selection and composing region, so let's notify it about updates.
      stateUpdated();
      immUpdateSelection();
      restartInput();
      return true;
    }
    return false;
  }

  // From BaseInputConnection
  @Override
  public Editable getEditable() {
    log("getEditable");
    return mEditable;
  }

  // From BaseInputConnection
  @Override
  public boolean setSelection(int start, int end) {
    log("setSelection: " + start + ":" + end);
    return super.setSelection(start, end);
  }

  // From BaseInputConnection
  @Override
  public boolean setComposingText(CharSequence text, int newCursorPosition) {
    log(
        "setComposingText='" + text + "' newCursorPosition=" + newCursorPosition);
    if (text == null) {
      return false;
    }
    return super.setComposingText(text, newCursorPosition);
  }

  @Override
  public boolean setComposingRegion(int start, int end) {
    log("setComposingRegion: " + start + ":" + end);
    return super.setComposingRegion(start, end);
  }

  // From BaseInputConnection
  @Override
  public boolean finishComposingText() {
    log("finishComposingText");
    return super.finishComposingText();
  }

  @Override
  public boolean endBatchEdit() {
    log("endBatchEdit");
    stateUpdated();
    return super.endBatchEdit();
  }

  @Override
  public boolean commitCompletion(CompletionInfo text) {
    log("commitCompletion");
    return super.commitCompletion(text);
  }

  @Override
  public boolean commitCorrection(CorrectionInfo text) {
    log("commitCompletion");
    return super.commitCorrection(text);
  }

  // From BaseInputConnection
  @Override
  public boolean commitText(CharSequence text, int newCursorPosition) {
      log(
        (new StringBuilder())
            .append("commitText: ")
            .append(text)
            .append(", new pos = ")
            .append(newCursorPosition)
            .toString());
    return super.commitText(text, newCursorPosition);
  }

  // From BaseInputConnection
  @Override
  public boolean deleteSurroundingText(int beforeLength, int afterLength) {
    log("deleteSurroundingText: " + beforeLength + ":" + afterLength);
    return super.deleteSurroundingText(beforeLength, afterLength);
  }

  // From BaseInputConnection
  @Override
  public boolean deleteSurroundingTextInCodePoints(int beforeLength, int afterLength) {
    log("deleteSurroundingTextInCodePoints: " + beforeLength + ":" + afterLength);
    return super.deleteSurroundingTextInCodePoints(beforeLength, afterLength);
  }

  // From BaseInputConnection
  @Override
  public boolean sendKeyEvent(KeyEvent event) {
    log("sendKeyEvent: " + event);
    return super.sendKeyEvent(event);
  }

  // From BaseInputConnection
  @Override
  public CharSequence getSelectedText(int flags) {
    CharSequence result = super.getSelectedText(flags);
    if (result == null) {
      result = "";
    }
    log("getSelectedText: " + flags + ", result: " + result);
    return result;
  }

  // From BaseInputConnection
  @Override
  public CharSequence getTextAfterCursor(int length, int flags) {
    log("getTextAfterCursor: " + length + ":" + flags);
    if (length < 0) {
      Log.i("darkfi", "getTextAfterCursor: returning null to due to an invalid length=" + length);
      return null;
    }
    return super.getTextAfterCursor(length, flags);
  }

  // From BaseInputConnection
  @Override
  public CharSequence getTextBeforeCursor(int length, int flags) {
    log("getTextBeforeCursor: " + length + ", flags=" + flags);
    if (length < 0) {
      Log.i("darkfi", "getTextBeforeCursor: returning null to due to an invalid length=" + length);
      return null;
    }
    return super.getTextBeforeCursor(length, flags);
  }

  // From BaseInputConnection
  @Override
  public boolean requestCursorUpdates(int cursorUpdateMode) {
    log("Request cursor updates: " + cursorUpdateMode);
    return super.requestCursorUpdates(cursorUpdateMode);
  }

  // From BaseInputConnection
  @Override
  public void closeConnection() {
    log("closeConnection");
    super.closeConnection();
  }

  @Override
  public boolean setImeConsumesInput(boolean imeConsumesInput) {
    log("setImeConsumesInput: " + imeConsumesInput);
    return super.setImeConsumesInput(imeConsumesInput);
  }

  @Override
  public ExtractedText getExtractedText(ExtractedTextRequest request, int flags) {
    log("getExtractedText");
    return super.getExtractedText(request, flags);
  }

  @Override
  public boolean performPrivateCommand(String action, Bundle data) {
    log("performPrivateCommand");
    return super.performPrivateCommand(action, data);
  }

  private void immUpdateSelection() {
    Pair selection = this.getSelection();
    Pair cr = this.getComposingRegion();
    log(
        "immUpdateSelection: " + selection.first + "," + selection.second + ". " + cr.first + ","
            + cr.second);
    settings.mEditorInfo.initialSelStart = selection.first;
    settings.mEditorInfo.initialSelEnd = selection.second;
    imm.updateSelection(targetView, selection.first, selection.second, cr.first, cr.second);
  }

  private Pair getSelection() {
    return new Pair(Selection.getSelectionStart(mEditable), Selection.getSelectionEnd(mEditable));
  }

  private Pair getComposingRegion() {
    return new Pair(getComposingSpanStart(mEditable), getComposingSpanEnd(mEditable));
  }

  private boolean processKeyEvent(KeyEvent event) {
    if (event == null) {
      return false;
    }
    int keyCode = event.getKeyCode();
    log(
        "processKeyEvent(key=" + keyCode + ") text=" + this.mEditable.toString());
    // Filter out Enter keys if multi-line mode is disabled.
    if ((settings.mEditorInfo.inputType & EditorInfo.TYPE_TEXT_FLAG_MULTI_LINE) == 0
        && (keyCode == KeyEvent.KEYCODE_ENTER || keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER)
        && event.hasNoModifiers()) {
      sendEditorAction(settings.mEditorInfo.actionId);
      return true;
    }
    if (event.getAction() != KeyEvent.ACTION_DOWN) {
      return false;
    }
    // If no selection is set, move the selection to the end.
    // This is the case when first typing on keys when the selection is not set.
    // Note that for InputType.TYPE_CLASS_TEXT, this is not be needed because the
    // selection is set in setComposingText.
    Pair selection = this.getSelection();
    if (selection.first == -1) {
      selection.first = this.mEditable.length();
      selection.second = this.mEditable.length();
    }

    if (keyCode == KeyEvent.KEYCODE_DPAD_LEFT) {
      if (selection.first == selection.second) {
        int newIndex = findIndexBackward(mEditable, selection.first, 1);
        setSelection(newIndex, newIndex);
      } else {
        setSelection(selection.first, selection.first);
      }
      return true;
    }
    if (keyCode == KeyEvent.KEYCODE_DPAD_RIGHT) {
      if (selection.first == selection.second) {
        int newIndex = findIndexForward(mEditable, selection.second, 1);
        setSelection(newIndex, newIndex);
      } else {
        setSelection(selection.second, selection.second);
      }
      return true;
    }
    if (keyCode == KeyEvent.KEYCODE_MOVE_HOME) {
      setSelection(0, 0);
      return true;
    }
    if (keyCode == KeyEvent.KEYCODE_MOVE_END) {
      setSelection(this.mEditable.length(), this.mEditable.length());
      return true;
    }
    if (keyCode == KeyEvent.KEYCODE_DEL || keyCode == KeyEvent.KEYCODE_FORWARD_DEL) {
      if (selection.first != selection.second) {
        this.mEditable.delete(selection.first, selection.second);
        return true;
      }
      if (keyCode == KeyEvent.KEYCODE_DEL) {
        if (selection.first > 0) {
          finishComposingText();
          deleteSurroundingTextInCodePoints(1, 0);
          return true;
        }
      }
      if (keyCode == KeyEvent.KEYCODE_FORWARD_DEL) {
        if (selection.first < this.mEditable.length()) {
          finishComposingText();
          deleteSurroundingTextInCodePoints(0, 1);
          return true;
        }
      }
      return false;
    }

    if (event.getUnicodeChar() == 0) {
      return false;
    }

    if (selection.first != selection.second) {
      log("processKeyEvent: deleting selection");
      this.mEditable.delete(selection.first, selection.second);
    }

    String charsToInsert = Character.toString((char) event.getUnicodeChar());
    this.mEditable.insert(selection.first, (CharSequence) charsToInsert);
    int length = this.mEditable.length();

    // Same logic as in setComposingText(): we must update composing region,
    // so make sure it points to a valid range.
    Pair composingRegion = this.getComposingRegion();
    if (composingRegion.first == -1) {
      composingRegion = this.getSelection();
      if (composingRegion.first == -1) {
        composingRegion = new Pair(0, 0);
      }
    }

    composingRegion.second = composingRegion.first + length;
    this.setComposingRegion(composingRegion.first, composingRegion.second);
    int new_cursor = selection.first + charsToInsert.length();
    setSelection(new_cursor, new_cursor);
    log("processKeyEvent: exit, text=" + this.mEditable.toString());
    return true;
  }

  private final void stateUpdated() {
    Pair selection = this.getSelection();
    Pair cr = this.getComposingRegion();
    State state = new State(
        this.mEditable.toString(), selection.first, selection.second, cr.first, cr.second);
    settings.mEditorInfo.initialSelStart = selection.first;
    settings.mEditorInfo.initialSelEnd = selection.second;

    // Keep a reference to the listener to avoid a race condition when setting the listener.
    Listener listener = this.listener;

    // We always propagate state change events because unfortunately keyboard visibility functions
    // are unreliable, and text editor logic should not depend on them.
    if (listener != null) {
      listener.stateChanged(state, /*dismissed=*/false);
    }
  }

  /**
   * Get the current IME insets.
   *
   * @return The current IME insets
   */
  public Insets getImeInsets() {
    if (this.targetView == null) {
      return Insets.NONE;
    }

    WindowInsets insets = this.targetView.getRootWindowInsets();

    if (insets == null) {
      return Insets.NONE;
    }

    return insets.getInsets(WindowInsets.Type.ime());
  }

  /**
   * Returns true if software keyboard is visible, false otherwise.
   *
   * @return whether software IME is visible or not.
   */
  public boolean isSoftwareKeyboardVisible() {
    if (this.targetView == null) {
      return false;
    }

    WindowInsets insets = this.targetView.getRootWindowInsets();

    if (insets == null) {
      return false;
    }

    return insets.isVisible(WindowInsets.Type.ime());
  }

  /**
   * This is an event handler from InputConnection interface.
   * It's called when action button is triggered (typically this means Enter was pressed).
   *
   * @param action Action code, either one from EditorInfo.imeOptions or a custom one.
   * @return Returns true on success, false if the input connection is no longer valid.
   */
  @Override
  public boolean performEditorAction(int action) {
    log("performEditorAction, action=" + action);
    if (action == IME_ACTION_UNSPECIFIED) {
      // Super emulates Enter key press/release
      return super.performEditorAction(action);
    }
    return sendEditorAction(action);
  }

  /**
   * Delivers editor action to listener
   *
   * @param action Action code, either one from EditorInfo.imeOptions or a custom one.
   * @return Returns true on success, false if the input connection is no longer valid.
   */
  private boolean sendEditorAction(int action) {
    Listener listener = this.listener;
    if (listener != null) {
      listener.onEditorAction(action);
      return true;
    }
    return false;
  }

  private static int INVALID_INDEX = -1;
  // Implementation copy from BaseInputConnection
  private static int findIndexBackward(
      final CharSequence cs, final int from, final int numCodePoints) {
    int currentIndex = from;
    boolean waitingHighSurrogate = false;
    final int N = cs.length();
    if (currentIndex < 0 || N < currentIndex) {
      return INVALID_INDEX; // The starting point is out of range.
    }
    if (numCodePoints < 0) {
      return INVALID_INDEX; // Basically this should not happen.
    }
    int remainingCodePoints = numCodePoints;
    while (true) {
      if (remainingCodePoints == 0) {
        return currentIndex; // Reached to the requested length in code points.
      }

      --currentIndex;
      if (currentIndex < 0) {
        if (waitingHighSurrogate) {
          return INVALID_INDEX; // An invalid surrogate pair is found.
        }
        return 0; // Reached to the beginning of the text w/o any invalid surrogate pair.
      }
      final char c = cs.charAt(currentIndex);
      if (waitingHighSurrogate) {
        if (!java.lang.Character.isHighSurrogate(c)) {
          return INVALID_INDEX; // An invalid surrogate pair is found.
        }
        waitingHighSurrogate = false;
        --remainingCodePoints;
        continue;
      }
      if (!java.lang.Character.isSurrogate(c)) {
        --remainingCodePoints;
        continue;
      }
      if (java.lang.Character.isHighSurrogate(c)) {
        return INVALID_INDEX; // A invalid surrogate pair is found.
      }
      waitingHighSurrogate = true;
    }
  }

  // Implementation copy from BaseInputConnection
  private static int findIndexForward(
      final CharSequence cs, final int from, final int numCodePoints) {
    int currentIndex = from;
    boolean waitingLowSurrogate = false;
    final int N = cs.length();
    if (currentIndex < 0 || N < currentIndex) {
      return INVALID_INDEX; // The starting point is out of range.
    }
    if (numCodePoints < 0) {
      return INVALID_INDEX; // Basically this should not happen.
    }
    int remainingCodePoints = numCodePoints;

    while (true) {
      if (remainingCodePoints == 0) {
        return currentIndex; // Reached to the requested length in code points.
      }

      if (currentIndex >= N) {
        if (waitingLowSurrogate) {
          return INVALID_INDEX; // An invalid surrogate pair is found.
        }
        return N; // Reached to the end of the text w/o any invalid surrogate pair.
      }
      final char c = cs.charAt(currentIndex);
      if (waitingLowSurrogate) {
        if (!java.lang.Character.isLowSurrogate(c)) {
          return INVALID_INDEX; // An invalid surrogate pair is found.
        }
        --remainingCodePoints;
        waitingLowSurrogate = false;
        ++currentIndex;
        continue;
      }
      if (!java.lang.Character.isSurrogate(c)) {
        --remainingCodePoints;
        ++currentIndex;
        continue;
      }
      if (java.lang.Character.isLowSurrogate(c)) {
        return INVALID_INDEX; // A invalid surrogate pair is found.
      }
      waitingLowSurrogate = true;
      ++currentIndex;
    }
  }
}
