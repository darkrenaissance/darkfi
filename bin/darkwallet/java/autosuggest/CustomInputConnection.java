/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

import android.util.Log;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.inputmethodservice.InputMethodService;

// setComposingText() - change text being composed

// See 30b299cb04b4ba2330ef61a8a24c1e58513a0af2
// content/public/android/java/src/org/chromium/content/browser/input/AdapterInputConnection.java

public class CustomInputConnection extends BaseInputConnection {

    native static void setup();
    native static void onCommitText(String text);
    native static void onEndEdit(String text);

    // Android is sending commit("foo") then edit("foo") events which is confusing.
    // We use this to skip edit("foo") when proceeded by the commit.
    private String lastCommitText;

    public CustomInputConnection(View view, boolean fullEditor) {
        super(view, fullEditor);
        lastCommitText = null;
        setup();
    }

    @Override
    public boolean sendKeyEvent(KeyEvent event) {
        int action = event.getAction();
        int keycode = event.getKeyCode();
        // If this is backspace/del or if the key has a character representation,
        // need to update the underlying Editable (i.e. the local representation of the text
        // being edited).  Some IMEs like Jellybean stock IME and Samsung IME mix in delete
        // KeyPress events instead of calling deleteSurroundingText.
        if (action == KeyEvent.ACTION_DOWN && keycode == KeyEvent.KEYCODE_DEL) {
            deleteSurroundingText(1, 0);

            //String text = getTextBeforeCursor(100, 0).toString();
            //text = text.substring(0, text.length() - 1);
            //setComposingText(text, 1);
        } else if (action == KeyEvent.ACTION_DOWN && keycode == KeyEvent.KEYCODE_FORWARD_DEL) {
            deleteSurroundingText(0, 1);
        } else if (action == KeyEvent.ACTION_DOWN && keycode == KeyEvent.KEYCODE_ENTER) {
            reset();
        }
        return true;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        //Log.i("darkfi", String.format("commitText(%s, %d)", text.toString(), newCursorPosition));
        lastCommitText = text.toString();
        onCommitText(lastCommitText);
        return super.commitText(text, newCursorPosition);
    }

    @Override
    public boolean endBatchEdit() {
        //Log.i("darkfi", "endBatchEdit: " + curr);
        String text = getTextBeforeCursor(100, 0).toString();
        if (!text.equals(lastCommitText))
            onEndEdit(text);
        lastCommitText = null;
        return super.endBatchEdit();
    }

    public void reset() {
        setComposingText("", 0);

        // Chromium does this but the above seems to work too.
        //beginBatchEdit();
        //finishComposingText();
        //endBatchEdit();
    }
}

