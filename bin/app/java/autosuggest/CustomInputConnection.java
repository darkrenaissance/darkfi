/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

package autosuggest;

import android.content.Context;
import android.os.Bundle;
import android.os.Handler;
import android.text.Editable;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.util.Log;
import android.view.KeyEvent;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputContentInfo;
import android.view.inputmethod.EditorInfo;
import android.view.View;
import android.view.inputmethod.CompletionInfo;
import android.view.inputmethod.CorrectionInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.SurroundingText;
import android.view.inputmethod.InputMethodManager;
//import android.view.inputmethod.TextSnapshot;
//import android.view.inputmethod.TextAttribute;

// This InputConnection is created by ContentView.onCreateInputConnection.
// It then adapts android's IME to chrome's RenderWidgetHostView using the
// native ImeAdapterAndroid via the outer class ImeAdapter.
public class CustomInputConnection extends BaseInputConnection {
    private static final boolean DEBUG = false;
    private int id = -1;

    private View mInternalView;
    //private ImeAdapter mImeAdapter;
    private Editable mEditable;
    private boolean mSingleLine;
    private int numBatchEdits;
    private boolean shouldUpdateImeSelection;

    native static void onCompose(int id, String text, int newCursorPos, boolean isCommit);
    native static void onSetComposeRegion(int id, int start, int end);
    native static void onFinishCompose(int id);
    native static void onDeleteSurroundingText(int id, int left, int right);

    //private AdapterInputConnection(View view, ImeAdapter imeAdapter, EditorInfo outAttrs) {
    public CustomInputConnection(int id, View view, EditorInfo outAttrs) {
        super(view, true);
        this.id = id;
        log("CustomInputConnection()");
        mInternalView = view;
        //mImeAdapter = imeAdapter;
        //mImeAdapter.setInputConnection(this);
        mSingleLine = true;
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN;
        outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
                | EditorInfo.TYPE_TEXT_VARIATION_WEB_EDIT_TEXT;
            /*
        if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeText) {
        */
            // Normal text field
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_GO;
            /*
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeTextArea ||
                imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeContentEditable) {
            // TextArea or contenteditable.
            outAttrs.inputType |= EditorInfo.TYPE_TEXT_FLAG_MULTI_LINE
                    | EditorInfo.TYPE_TEXT_FLAG_CAP_SENTENCES
                    | EditorInfo.TYPE_TEXT_FLAG_AUTO_CORRECT;
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_NONE;
            mSingleLine = false;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypePassword) {
            // Password
            outAttrs.inputType = InputType.TYPE_CLASS_TEXT
                    | InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD;
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_GO;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeSearch) {
            // Search
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_SEARCH;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeUrl) {
            // Url
            // TYPE_TEXT_VARIATION_URI prevents Tab key from showing, so
            // exclude it for now.
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_GO;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeEmail) {
            // Email
            outAttrs.inputType = InputType.TYPE_CLASS_TEXT
                    | InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS;
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_GO;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeTel) {
            // Telephone
            // Number and telephone do not have both a Tab key and an
            // action in default OSK, so set the action to NEXT
            outAttrs.inputType = InputType.TYPE_CLASS_PHONE;
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_NEXT;
        } else if (imeAdapter.mTextInputType == ImeAdapter.sTextInputTypeNumber) {
            // Number
            outAttrs.inputType = InputType.TYPE_CLASS_NUMBER
                    | InputType.TYPE_NUMBER_VARIATION_NORMAL;
            outAttrs.imeOptions |= EditorInfo.IME_ACTION_NEXT;
        }
        */
    }

    private void log(String fstr, Object... args) {
        if (!DEBUG) return;
        String text = "(" + id + "): " + String.format(fstr, args);
        Log.d("darkfi", text);
    }

    /**
     * Updates the AdapterInputConnection's internal representation of the text
     * being edited and its selection and composition properties. The resulting
     * Editable is accessible through the getEditable() method.
     * If the text has not changed, this also calls updateSelection on the InputMethodManager.
     * @param text The String contents of the field being edited
     * @param selectionStart The character offset of the selection start, or the caret
     * position if there is no selection
     * @param selectionEnd The character offset of the selection end, or the caret
     * position if there is no selection
     * @param compositionStart The character offset of the composition start, or -1
     * if there is no composition
     * @param compositionEnd The character offset of the composition end, or -1
     * if there is no selection
     */
    public void setEditableText(String text, int selectionStart, int selectionEnd,
            int compositionStart, int compositionEnd) {
        log("setEditableText(%s, %d, %d, %d, %d)", text,
            selectionStart, selectionEnd,
            compositionStart, compositionEnd);

        if (mEditable == null) {
            log("setEditableText creating new editable");
            mEditable = Editable.Factory.getInstance().newEditable("");
        }

        int prevSelectionStart = Selection.getSelectionStart(mEditable);
        int prevSelectionEnd = Selection.getSelectionEnd(mEditable);
        int prevEditableLength = mEditable.length();
        int prevCompositionStart = getComposingSpanStart(mEditable);
        int prevCompositionEnd = getComposingSpanEnd(mEditable);
        String prevText = mEditable.toString();

        selectionStart = Math.min(selectionStart, text.length());
        selectionEnd = Math.min(selectionEnd, text.length());
        compositionStart = Math.min(compositionStart, text.length());
        compositionEnd = Math.min(compositionEnd, text.length());

        boolean textUnchanged = prevText.equals(text);

        if (textUnchanged
                && prevSelectionStart == selectionStart && prevSelectionEnd == selectionEnd
                && prevCompositionStart == compositionStart
                && prevCompositionEnd == compositionEnd) {
            // Nothing has changed; don't need to do anything
            return;
        }

        // When a programmatic change has been made to the editable field, both the start
        // and end positions for the composition will equal zero. In this case we cancel the
        // active composition in the editor as this no longer is relevant.
        if (textUnchanged && compositionStart == 0 && compositionEnd == 0) {
            cancelComposition();
        }

        if (!textUnchanged) {
            log("replace mEditable with: %s", text);
            mEditable.replace(0, mEditable.length(), text);
        }
        Selection.setSelection(mEditable, selectionStart, selectionEnd);
        super.setComposingRegion(compositionStart, compositionEnd);

        if (textUnchanged || prevText.equals("")) {
            log("setEditableText updating selection");
            // updateSelection should be called when a manual selection change occurs.
            // Should not be called if text is being entered else issues can occur
            // e.g. backspace to undo autocorrection will not work with the default OSK.
            getInputMethodManager().updateSelection(mInternalView,
                    selectionStart, selectionEnd, compositionStart, compositionEnd);
        }
    }

    @Override
    public Editable getEditable() {
        if (mEditable == null) {
            log("getEditable() [create new]");
            mEditable = Editable.Factory.getInstance().newEditable("");
            Selection.setSelection(mEditable, 0);
        }
        log("getEditable() -> %s", editableToXml(mEditable));
        return mEditable;
    }

    @Override
    public boolean setComposingText(CharSequence text, int newCursorPosition) {
        log("setComposingText(%s, %d)", text, newCursorPosition);
        super.setComposingText(text, newCursorPosition);
        shouldUpdateImeSelection = true;
        onCompose(id, text.toString(), newCursorPosition, false);
        return true;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        log("commitText(%s, %d)", text, newCursorPosition);
        super.commitText(text, newCursorPosition);
        shouldUpdateImeSelection = true;
        onCompose(id, text.toString(), newCursorPosition, text.length() > 0);
        return true;
    }

    @Override
    public boolean performEditorAction(int actionCode) {
        log("performEditorAction(%d)", actionCode);
        switch (actionCode) {
            case EditorInfo.IME_ACTION_NEXT:
                cancelComposition();
                // Send TAB key event
                long timeStampMs = System.currentTimeMillis();
                //mImeAdapter.sendSyntheticKeyEvent(
                //        sEventTypeRawKeyDown, timeStampMs, KeyEvent.KEYCODE_TAB, 0);
                return true;
            case EditorInfo.IME_ACTION_GO:
            case EditorInfo.IME_ACTION_SEARCH:
                //mImeAdapter.dismissInput(true);
                break;
        }

        return super.performEditorAction(actionCode);
    }

    @Override
    public boolean performContextMenuAction(int id) {
        log("performContextMenuAction(%d)", id);
        /*
        switch (id) {
            case android.R.id.selectAll:
                return mImeAdapter.selectAll();
            case android.R.id.cut:
                return mImeAdapter.cut();
            case android.R.id.copy:
                return mImeAdapter.copy();
            case android.R.id.paste:
                return mImeAdapter.paste();
            default:
                return false;
        }
        */
        return false;
    }

    @Override
    public CharSequence getTextAfterCursor(int length, int flags) {
        log("getTextAfterCursor(%d, %d)", length, flags);
        return super.getTextAfterCursor(length, flags);
    }
    @Override
    public CharSequence getTextBeforeCursor(int length, int flags) {
        log("getTextBeforeCursor(%d, %d)", length, flags);
        return super.getTextBeforeCursor(length, flags);
    }
    @Override
    public SurroundingText getSurroundingText(int beforeLength, int afterLength, int flags) {
        log("getSurroundingText(%d, %d, %d)", beforeLength, afterLength, flags);
        return super.getSurroundingText(beforeLength, afterLength, flags);
    }
    @Override
    public CharSequence getSelectedText(int flags) {
        log("getSelectedText(%d)", flags);
        return super.getSelectedText(flags);
    }

    @Override
    public ExtractedText getExtractedText(ExtractedTextRequest request, int flags) {
        log("getExtractedText(...)");
        ExtractedText et = new ExtractedText();
        if (mEditable == null) {
            et.text = "";
        } else {
            et.text = mEditable.toString();
            et.partialEndOffset = mEditable.length();
            et.selectionStart = Selection.getSelectionStart(mEditable);
            et.selectionEnd = Selection.getSelectionEnd(mEditable);
        }
        et.flags = mSingleLine ? ExtractedText.FLAG_SINGLE_LINE : 0;
        return et;
    }

    @Override
    public boolean deleteSurroundingText(int leftLength, int rightLength) {
        log("deleteSurroundingText(%d, %d)", leftLength, rightLength);
        if (!super.deleteSurroundingText(leftLength, rightLength)) {
            return false;
        }
        shouldUpdateImeSelection = true;
        //return mImeAdapter.deleteSurroundingText(leftLength, rightLength);
        onDeleteSurroundingText(id, leftLength, rightLength);
        return true;
    }

    @Override
    public boolean sendKeyEvent(KeyEvent event) {
        int action = event.getAction();
        int keycode = event.getKeyCode();
        log("sendKeyEvent()  [action=%d, keycode=%d]", action, keycode);

        //mImeAdapter.mSelectionHandleController.hideAndDisallowAutomaticShowing();
        //mImeAdapter.mInsertionHandleController.hideAndDisallowAutomaticShowing();

        // If this is a key-up, and backspace/del or if the key has a character representation,
        // need to update the underlying Editable (i.e. the local representation of the text
        // being edited).
        if (event.getAction() == KeyEvent.ACTION_UP) {
            if (event.getKeyCode() == KeyEvent.KEYCODE_DEL) {
                super.deleteSurroundingText(1, 0);
            } else if (event.getKeyCode() == KeyEvent.KEYCODE_FORWARD_DEL) {
                super.deleteSurroundingText(0, 1);
            } else {
                int unicodeChar = event.getUnicodeChar();
                if (unicodeChar != 0) {
                    Editable editable = getEditable();
                    int selectionStart = Selection.getSelectionStart(editable);
                    int selectionEnd = Selection.getSelectionEnd(editable);
                    if (selectionStart > selectionEnd) {
                        int temp = selectionStart;
                        selectionStart = selectionEnd;
                        selectionEnd = temp;
                    }
                    editable.replace(selectionStart, selectionEnd,
                            Character.toString((char)unicodeChar));
                }
            }
        }
        shouldUpdateImeSelection = true;
        return super.sendKeyEvent(event);
    }

    @Override
    public boolean finishComposingText() {
        log("finishComposingText()");
        if (mEditable == null
                || (getComposingSpanStart(mEditable) == getComposingSpanEnd(mEditable))) {
            return true;
        }
        super.finishComposingText();
        onFinishCompose(id);
        return true;
    }

    @Override
    public boolean setSelection(int start, int end) {
        log("setSelection(%d, %d)", start, end);
        if (start < 0 || end < 0) return true;
        super.setSelection(start, end);
        shouldUpdateImeSelection = true;
        //return mImeAdapter.setEditableSelectionOffsets(start, end);
        return true;
    }

    /**
     * Informs the InputMethodManager and InputMethodSession (i.e. the IME) that there
     * is no longer a current composition. Note this differs from finishComposingText, which
     * is called by the IME when it wants to end a composition.
     */
    void cancelComposition() {
        log("cancelComposition()");
        getInputMethodManager().restartInput(mInternalView);
    }

    @Override
    public boolean setComposingRegion(int start, int end) {
        log("setComposingRegion(%d, %d)", start, end);
        int a = Math.min(start, end);
        int b = Math.max(start, end);
        super.setComposingRegion(a, b);
        onSetComposeRegion(id, a, b);
        return true;
    }

    boolean isActive() {
        return getInputMethodManager().isActive();
    }

    private InputMethodManager getInputMethodManager() {
        InputMethodManager imm = (InputMethodManager)mInternalView.getContext()
            .getSystemService(Context.INPUT_METHOD_SERVICE);
        if (imm == null) {
            Log.e("darkfi", "[IC]: InputMethodManager is NULL!");
        }
        return imm;
    }

    private void updateImeSelection() {
        log("updateImeSelection()");
        if (mEditable == null) {
            return;
        }

        getInputMethodManager().updateSelection(
            mInternalView,
            Selection.getSelectionStart(mEditable),
            Selection.getSelectionEnd(mEditable),
            getComposingSpanStart(mEditable),
            getComposingSpanEnd(mEditable)
        );
        log("updateImeSelection() DONE");
    }

    @Override
    public boolean beginBatchEdit() {
        log("beginBatchEdit");
        ++numBatchEdits;
        return false;
    }

    @Override
    public boolean endBatchEdit() {
        log("endBatchEdit");
        if (--numBatchEdits == 0 && shouldUpdateImeSelection) {
            updateImeSelection();
            shouldUpdateImeSelection = false;
        }
        log("endBatchEdit DONE");
        return false;
    }

    private String editableToXml(Editable editable) {
        StringBuilder xmlBuilder = new StringBuilder();
        int length = editable.length();

        Object[] spans = editable.getSpans(0, editable.length(), Object.class);

        for (int i = 0; i < length; i++) {
            // Find spans starting at this position
            for (Object span : spans) {
                if (editable.getSpanStart(span) == i) {
                    xmlBuilder
                        .append("<")
                        .append(span.getClass().getSimpleName())
                        .append(">");
                }
            }

            // Append the character
            char c = editable.charAt(i);
            xmlBuilder.append(c);

            if (Character.isHighSurrogate(c)) {
                if (i + 1 < editable.length() && Character.isLowSurrogate(editable.charAt(i + 1))) {
                    i += 1;
                    xmlBuilder.append(editable.charAt(i));
                }
            }

            // Find spans ending at this position
            for (Object span : spans) {
                if (editable.getSpanEnd(span) == i) {
                    xmlBuilder
                        .append("</")
                        .append(span.getClass().getSimpleName())
                        .append(">");
                }
            }
        }

        // Find spans starting at this position
        for (Object span : spans) {
            if (editable.getSpanStart(span) == length) {
                xmlBuilder
                    .append("<")
                    .append(span.getClass().getSimpleName())
                    .append(">");
            }
        }
        // Find spans ending at this position
        for (Object span : spans) {
            if (editable.getSpanEnd(span) == length) {
                xmlBuilder
                    .append("</")
                    .append(span.getClass().getSimpleName())
                    .append(">");
            }
        }

        return xmlBuilder.toString();
    }

    public String debugEditableStr() {
        return editableToXml(mEditable);
    }
    public String rawText() {
        return mEditable.toString();
    }
    public int getSelectionStart() {
        return Selection.getSelectionStart(mEditable);
    }
    public int getSelectionEnd() {
        return Selection.getSelectionEnd(mEditable);
    }
    public int getComposeStart() {
        return getComposingSpanStart(mEditable);
    }
    public int getComposeEnd() {
        return getComposingSpanEnd(mEditable);
    }
}

